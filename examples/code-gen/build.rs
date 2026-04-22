use std::{
    env,
    fs,
    path::PathBuf,
};

use hashbrown::HashMap;
use syn::{
    GenericArgument,
    Item,
    ItemFn,
    PathArguments,
    ReturnType,
    Type,
    spanned::Spanned,
    visit::{
        self,
        Visit,
    },
};

// --- Data Structures ---

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct CompositeErrorDefinition {
    plain_types: Vec<String>,
    composite_types: Vec<String>,
    file: String,
    line: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct ErrorDefinition {
    file: String,
    line: usize,
}

enum DefFailCause {
    NameConflict {
        name: String,
    },
    WrongErrorExpansion {
        missing_plain_errors: Vec<String>,
        missing_composite_errors: Vec<String>,
    },
    NotInResult,
}

struct ErrorDefinitionFail {
    cause: DefFailCause,
    file: String,
    line: usize,
}

struct SkerryScanner<'a> {
    file_path: &'a str,
    type_definitions: &'a mut HashMap<String, ErrorDefinition>,
    composite_definitions: &'a mut HashMap<String, CompositeErrorDefinition>,
    errors: &'a mut Vec<ErrorDefinitionFail>,
}

impl<'a> Visit<'a> for SkerryScanner<'a> {
    fn visit_item(&mut self, i: &'a Item) {
        let attrs = match i {
            Item::Struct(s) => (&s.attrs, &s.ident, s.struct_token.span),
            Item::Enum(e) => (&e.attrs, &e.ident, e.enum_token.span),
            _ => {
                visit::visit_item(self, i);
                return;
            }
        };

        let (attrs, ident, _) = attrs;
        if let Some(attr) = attrs.iter().find_map(|attr| {
            if attr.path().is_ident("skerry_error") {
                Some(attr)
            } else {
                None
            }
        }) {
            if self
                .type_definitions
                .try_insert(
                    ident.to_string(),
                    ErrorDefinition {
                        file: self.file_path.to_string(),
                        line: attr.span().start().line,
                    },
                )
                .is_err()
            {
                self.errors.push(ErrorDefinitionFail {
                    cause: DefFailCause::NameConflict {
                        name: ident.to_string(),
                    },
                    file: self.file_path.to_string(),
                    line: attr.span().start().line,
                });
            }
        }

        visit::visit_item(self, i);
    }

    fn visit_item_fn(&mut self, i: &'a ItemFn) {
        // Look for Result<T, e![...]>
        if let ReturnType::Type(_, ty) = &i.sig.output {
            if let Some((plain_types, composite_types)) = extract_skerry_macro_types(ty) {
                let raw_name = i.sig.ident.to_string();
                let composite_name =
                    format!("{}{}Error", &raw_name[..1].to_uppercase(), &raw_name[1..]);

                if self
                    .composite_definitions
                    .try_insert(
                        composite_name.clone(),
                        CompositeErrorDefinition {
                            plain_types,
                            composite_types,
                            file: self.file_path.to_string(),
                            line: ty.span().start().line,
                        },
                    )
                    .is_err()
                {
                    self.errors.push(ErrorDefinitionFail {
                        cause: DefFailCause::NameConflict {
                            name: composite_name,
                        },
                        file: self.file_path.to_string(),
                        line: ty.span().start().line,
                    });
                }
            } else {
                self.errors.push(ErrorDefinitionFail {
                    cause: DefFailCause::NotInResult,
                    file: self.file_path.to_string(),
                    line: ty.span().start().line,
                });
            }
        }
        visit::visit_item_fn(self, i);
    }
}

// Helper to extract types from Result<_, e![Type1, *Type2]>
fn extract_skerry_macro_types(ty: &Type) -> Option<(Vec<String>, Vec<String>)> {
    let path = match ty {
        Type::Path(tp) => &tp.path,
        _ => return None,
    };

    let last_seg = path.segments.last()?;
    if last_seg.ident != "Result" {
        return None;
    }

    // Get the second generic argument: Result<T, E>
    if let PathArguments::AngleBracketed(args) = &last_seg.arguments {
        if let Some(GenericArgument::Type(Type::Macro(m))) = args.args.get(1) {
            if m.mac.path.segments.last()?.ident == "e" {
                let content = m.mac.tokens.to_string();
                let mut plains = Vec::new();
                let mut starred = Vec::new();

                for s in content.split(',') {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if trimmed.starts_with('*') {
                        starred.push(trimmed[1..].trim().to_string());
                    } else {
                        plains.push(trimmed.to_string());
                    }
                }
                return Some((plains, starred));
            }
        }
    }
    None
}

// --- Main ---

fn main() {
    println!("cargo:rerun-if-changed=src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut type_definitions = HashMap::new();
    let mut composite_definitions = HashMap::new();
    let mut failures: Vec<ErrorDefinitionFail> = Vec::new();

    for entry in walkdir::WalkDir::new("src")
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "rs") {
            let content = fs::read_to_string(path).unwrap_or_default();

            if !content.contains("e![") && !content.contains("#[skerry_error]") {
                continue;
            }

            let syntax_tree = match syn::parse_file(&content) {
                Ok(tree) => tree,
                Err(_) => continue, // Skip files with syntax errors
            };

            let mut scanner = SkerryScanner {
                file_path: path.to_str().unwrap_or("unknown"),
                type_definitions: &mut type_definitions,
                composite_definitions: &mut composite_definitions,
                errors: &mut failures,
            };

            visit::visit_file(&mut scanner, &syntax_tree);
        }
    }

    // --- Generation Logic ---

    let mut all_defs = Vec::new();
    let mut all_arms = Vec::new();

    for def in &composite_definitions {
        let mut missing_plain_errors = vec![];
        let mut missing_composite_errors = vec![];

        // Checking for missing plain types
        for plain_type in &def.1.plain_types {
            if !type_definitions.contains_key(plain_type) {
                missing_plain_errors.push(plain_type.clone());
            }
        }
        // Checking for missing composite types
        for composite_type in &def.1.composite_types {
            if !composite_definitions.contains_key(composite_type) {
                missing_composite_errors.push(composite_type.clone());
            }
        }

        if !(missing_plain_errors.is_empty() && missing_composite_errors.is_empty()) {
            failures.push(ErrorDefinitionFail {
                cause: DefFailCause::WrongErrorExpansion {
                    missing_plain_errors,
                    missing_composite_errors,
                },
                file: def.1.file.clone(),
                line: def.1.line,
            });
            continue;
        }

        let types_str = def.1.plain_types.join(",");

        all_defs.push(format!(
            "pub enum {} {{
            {}
        }}",
            def.0, types_str
        ));

        all_arms.push(format!(
            "    ({:?}, {}) => {{ {} }};",
            def.1.file, def.1.line, def.0
        ));
    }
    for plain_type in type_definitions {
        all_arms.push(format!(
            "    ({:?}, {}) => {{}};",
            plain_type.1.file, plain_type.1.line
        ));
    }
    for error in failures {
        let error_message = match error.cause {
            DefFailCause::NameConflict { name } => format!("Conflicting name definition: {}", name),
            DefFailCause::WrongErrorExpansion {
                missing_plain_errors,
                missing_composite_errors,
            } => format!(
                "Errors do not exist: {}, {}",
                missing_plain_errors.join(","),
                missing_composite_errors.join(",")
            ),
            DefFailCause::NotInResult => "e![] can only be used inside Result".to_string(),
        };
        all_arms.push(format!(
            "    ({:?}, {}) => {{ compile_error!(\"{}\") }};",
            error.file, error.line, error_message
        ));
    }

    let header = "/* GENERATED BY SKERRY CODEGEN */";
    let output = format!(
        "{}\n{defs}\n\n#[macro_export]\nmacro_rules! skerry_invoke {{\n{arms}\n    ($file:expr, $line:expr) => {{ compile_error!(concat!(\"Skerry Sync Error: No macro generated for \", $file, \":\", $line)); }};\n}}",
        header,
        defs = all_defs.join("\n"),
        arms = all_arms.join("\n")
    );

    fs::write(out_dir.join("skerry_gen.rs"), output).unwrap();
}
