use std::{
    env,
    fs,
    path::PathBuf,
};

use hashbrown::HashMap;
use quote::ToTokens;
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

pub enum ErrorDefinition {
    Simple {
        raw: String,
        file: String,
        line: usize,
    },
    Composite {
        types: Vec<String>,
        composites: Vec<String>,
        file: String,
        line: usize,
    },
}

enum DefFailCause {
    NameConflict {
        name: String,
    },
    WrongErrorExpansion {
        missing_errors: Vec<String>,
        remove_asterisk: Vec<String>,
        add_asterisk: Vec<String>,
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
    errors: &'a mut Vec<ErrorDefinitionFail>,
}

impl<'a> Visit<'a> for SkerryScanner<'a> {
    fn visit_item(&mut self, i: &'a Item) {
        let attrs = match i {
            Item::Struct(s) => {
                let ident = &s.ident;
                let mut s = s.clone();
                let attrs = std::mem::replace(&mut s.attrs, vec![]);
                (attrs, ident, s.to_token_stream().to_string())
            }
            Item::Enum(e) => {
                let ident = &e.ident;
                let mut e = e.clone();
                let attrs = std::mem::replace(&mut e.attrs, vec![]);
                (attrs, ident, e.to_token_stream().to_string())
            }
            _ => {
                visit::visit_item(self, i);
                return;
            }
        };

        let (attrs, ident, raw) = attrs;
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
                    ErrorDefinition::Simple {
                        raw,
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
        if let ReturnType::Type(_, ty) = &i.sig.output {
            if let Some((types, composites)) = extract_skerry_macro_types(ty) {
                let raw_name = i.sig.ident.to_string();
                let camel_case_name: String = raw_name
                    .split('_')
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                        }
                    })
                    .collect();
                let composite_name = format!("{}Error", camel_case_name);

                if self
                    .type_definitions
                    .try_insert(
                        composite_name.clone(),
                        ErrorDefinition::Composite {
                            types,
                            composites,
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
                let content: String = m.mac.tokens.to_string();

                let mut types = Vec::new();
                let mut composites = Vec::new();

                for s in content.split(',') {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if trimmed.starts_with('*') {
                        composites.push(trimmed[1..].trim().to_string());
                    } else {
                        types.push(trimmed.to_string());
                    }
                }

                return Some((types, composites));
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut type_definitions = HashMap::new();
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
                errors: &mut failures,
            };

            visit::visit_file(&mut scanner, &syntax_tree);
        }
    }

    let mut all_defs = Vec::new();
    let mut all_arms = Vec::new();

    let mut plain_defs = String::new();

    // Validate and generate errors
    for (name, def) in &type_definitions {
        match def {
            ErrorDefinition::Simple { raw, file, line } => {
                all_arms.push(format!(
                    "    ({:?}, {}) => {{ use crate::errors::{}; }};",
                    file, line, name
                ));

                plain_defs.push_str(&raw);
            }
            ErrorDefinition::Composite {
                types,
                composites,
                file,
                line,
            } => {
                let mut missing_errors = vec![];
                let mut remove_asterisk = vec![];
                let mut add_asterisk = vec![];

                // Checking for missing plain types
                for plain_type in types {
                    if let Some(t) = type_definitions.get(plain_type) {
                        if let ErrorDefinition::Composite { .. } = t {
                            add_asterisk.push(plain_type.clone());
                        }
                    } else {
                        missing_errors.push(plain_type.clone());
                    }
                }
                for composite in composites {
                    if let Some(t) = type_definitions.get(composite) {
                        if let ErrorDefinition::Simple { .. } = t {
                            remove_asterisk.push(composite.clone());
                        }
                    } else {
                        missing_errors.push(composite.clone());
                    }
                }

                if !(missing_errors.is_empty()
                    && remove_asterisk.is_empty()
                    && add_asterisk.is_empty())
                {
                    failures.push(ErrorDefinitionFail {
                        cause: DefFailCause::WrongErrorExpansion {
                            missing_errors,
                            remove_asterisk,
                            add_asterisk,
                        },
                        file: file.clone(),
                        line: *line,
                    });
                    continue;
                }

                let mut all_types = types.clone();
                let asterisked = composites.iter().map(|s| format!("*{}", s));
                all_types.extend(asterisked);

                all_defs.push(format!(
                    "skerry::define_error!({}, [{}]);",
                    &name,
                    all_types.join(",")
                ));

                all_arms.push(format!(
                    "    ({:?}, {}) => {{ crate::errors::{} }};",
                    file, line, &name
                ));
            }
        }
    }

    for error in failures {
        let error_message = match error.cause {
            DefFailCause::NameConflict { name } => format!("Conflicting name definition: {}", name),
            DefFailCause::WrongErrorExpansion {
                missing_errors,
                remove_asterisk,
                add_asterisk,
            } => {
                let mut lines = Vec::new();

                if !missing_errors.is_empty() {
                    lines.push(format!(
                        "The following types were not found: [{}]",
                        missing_errors.join(", ")
                    ));
                }

                if !add_asterisk.is_empty() {
                    lines.push(format!(
                        "Add '*' prefix to composite errors: [{}]",
                        add_asterisk.join(", ")
                    ));
                }

                if !remove_asterisk.is_empty() {
                    lines.push(format!(
                        "Remove the '*' on plain errors: [{}]",
                        remove_asterisk.join(", ")
                    ));
                }

                lines.join("\n")
            }
            DefFailCause::NotInResult => "e![] can only be used inside Result".to_string(),
        };

        all_arms.push(format!(
            "    ({file:?}, {line}) => {{ compile_error!(\"{file}:{line} - {msg}\") }};",
            file = error.file,
            line = error.line,
            msg = error_message
        ));
    }

    let header = "/* GENERATED BY SKERRY CODEGEN */\n";
    let output = format!(
        "{}\npub mod errors {{
    #[skerry::skerry_mod]
    mod auto {{
        {plain_defs}
    }}
    {defs}
    \n}}\n\n#[macro_export]\nmacro_rules! skerry_invoke {{\n{arms}\n    ($file:expr, $line:expr) => {{ compile_error!(concat!(\"Skerry Sync Error: No macro generated for \", $file, \":\", $line)); }};\n}}",
        header,
        plain_defs = plain_defs,
        defs = all_defs.join("\n"),
        arms = all_arms.join("\n")
    );

    fs::write(out_dir.join("skerry_gen.rs"), output).unwrap();
}
