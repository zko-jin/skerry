use std::{
    env,
    fs::{
        self,
    },
    io::Write,
    path::PathBuf,
    time::SystemTime,
};

use ahash::RandomState;
use hashbrown::{
    HashMap,
    hash_map::Entry,
};
use proc_macro2::TokenStream;
use quote::ToTokens;
use serde::{
    Deserialize,
    Serialize,
};
use syn::{
    Ident,
    Item,
    ItemImpl,
    ItemTrait,
    PathArguments,
    ReturnType,
    Token,
    Type,
    parse::ParseStream,
    visit::{
        self,
        Visit,
    },
};
use topological_sort::TopologicalSort;
pub use writer::WrittenResult;

use crate::writer::SkerryWriter;

mod writer;

// fn calculate_ident_hash(ident: &syn::Ident) -> u64 {
//     let hasher = RandomState::with_seeds(0, 0, 0, 0);
//     hasher.hash_one(ident.to_string())
// }

pub fn calculate_sig_hash(prefix: String, sig: &syn::Signature) -> u64 {
    let sig_string = sig.to_token_stream().to_string();
    let normalized: String = format!(
        "{}{}",
        prefix,
        sig_string
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    );

    let hasher = RandomState::with_seeds(0, 0, 0, 0);
    hasher.hash_one(normalized)
}

#[derive(Clone, Serialize, Deserialize)]
struct CompositeType {
    types: Vec<String>,
    composites: Vec<String>,
    file: String,
    hash: u64,
}

#[derive(Clone, Serialize, Deserialize)]
enum SimpleFields {
    Unit,
    Unnamed(Vec<UnnamedField>),
    Named(Vec<NamedField>),
}

#[derive(Clone, Serialize, Deserialize)]
struct NamedField {
    name: String,
    ty: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct UnnamedField {
    ty: String,
}

impl SimpleFields {
    pub fn display_expansion(&self) -> String {
        match self {
            SimpleFields::Unit => String::new(),
            SimpleFields::Unnamed(fields) => {
                let mut s = "(".to_string();
                for i in 0..fields.len() {
                    s.push_str("var_");
                    s.push_str(&i.to_string());
                    s.push(',');
                }
                s.push(')');
                s
            }
            SimpleFields::Named(fields) => {
                let mut s = "{".to_string();
                for f in fields {
                    s.push_str(&f.name);
                    s.push(',');
                }
                s.push('}');
                s
            }
        }
    }

    pub fn display_def(&self) -> String {
        match self {
            SimpleFields::Unit => String::new(),
            SimpleFields::Unnamed(fields) => {
                let mut s = "(".to_string();
                for f in fields {
                    s.push_str(&f.ty);
                    s.push(',');
                }
                s.push(')');
                s
            }
            SimpleFields::Named(fields) => {
                let mut s = "{".to_string();
                for f in fields {
                    s.push_str(&f.name);
                    s.push(':');
                    s.push_str(&f.ty);
                    s.push(',');
                }
                s.push('}');
                s
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct SimpleType {
    fields: SimpleFields,
    from: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
enum TypeDefinitionType {
    Simple(SimpleType),
    Composite(CompositeType),
}

#[derive(Clone, Serialize, Deserialize)]
struct TypeDefinition {
    ty: TypeDefinitionType,
}

impl TypeDefinition {
    pub fn simple(ty: SimpleType) -> Self {
        Self {
            ty: TypeDefinitionType::Simple(ty),
        }
    }

    pub fn composite(composite: CompositeType) -> Self {
        Self {
            ty: TypeDefinitionType::Composite(composite),
        }
    }
}

enum DefinitionErrorCause {
    NameConflict {
        name: String,
    },
    WrongErrorExpansion {
        missing_errors: Vec<String>,
        remove_asterisk: Vec<String>,
        add_asterisk: Vec<String>,
    },
}

impl DefinitionErrorCause {
    pub fn to_msg(self) -> String {
        match self {
            DefinitionErrorCause::NameConflict { name } => {
                format!("Conflicting name definition: {}", name)
            }
            DefinitionErrorCause::WrongErrorExpansion {
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
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TypeDefinitionError {
    msg: String,
    file: String,
    hash: u64,
}

impl TypeDefinitionError {
    pub fn new(cause: DefinitionErrorCause, file: String, hash: u64) -> Self {
        Self {
            msg: cause.to_msg(),
            file,
            hash,
        }
    }
}

struct SkerryScanner<'a> {
    file_path: &'a str,
    type_definitions: &'a mut HashMap<String, TypeDefinition>,
    errors: &'a mut Vec<TypeDefinitionError>,
    prefix_stack: Vec<String>,
    module_stack: Vec<String>,
    generator: &'a mut SkerryGenerator,
    global_error_path: &'a mut Option<(String, String)>,
}

#[allow(unused)]
enum ErrorInput {
    Standard(Ident),
    Spread(Ident),
}

fn extract_e_errors(output: &syn::ReturnType) -> Result<(Vec<String>, Vec<String>), ()> {
    enum ErrorItem {
        Simple(Ident),
        Composite(Ident),
    }

    impl syn::parse::Parse for ErrorItem {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            if input.peek(Token![*]) {
                input.parse::<Token![*]>()?;
                Ok(ErrorItem::Composite(input.parse()?))
            } else {
                Ok(ErrorItem::Simple(input.parse()?))
            }
        }
    }

    struct SkerryErrorList {
        simple: Vec<Ident>,
        composite: Vec<Ident>,
    }

    impl syn::parse::Parse for SkerryErrorList {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let items: syn::punctuated::Punctuated<ErrorItem, Token![,]> =
                input.parse_terminated(ErrorItem::parse, Token![,])?;

            if items.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "Should contain at least one element",
                ));
            }

            let mut simple = Vec::new();
            let mut composite = Vec::new();

            for item in items {
                match item {
                    ErrorItem::Simple(id) => simple.push(id),
                    ErrorItem::Composite(id) => composite.push(id),
                }
            }

            Ok(SkerryErrorList { simple, composite })
        }
    }
    match &output {
        ReturnType::Type(_, ty) => {
            let tokens = extract_result_error_tokens(ty)?;
            let errors: SkerryErrorList = syn::parse2(tokens).unwrap();
            Ok((
                errors.simple.into_iter().map(|e| e.to_string()).collect(),
                errors
                    .composite
                    .into_iter()
                    .map(|e| e.to_string())
                    .collect(),
            ))
        }
        ReturnType::Default => Err(()),
    }
}

fn extract_result_error_tokens(ty: &Type) -> Result<TokenStream, ()> {
    let path = match ty {
        Type::Path(tp) => &tp.path,
        _ => panic!(),
    };
    let last_segment = path.segments.last().unwrap();
    if last_segment.ident != "Result" {
        panic!();
    }

    let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
        panic!()
    };

    let macro_arg = args.args.get(1).unwrap();
    match macro_arg {
        syn::GenericArgument::Type(Type::Macro(m)) => Ok(m.mac.tokens.clone()),
        _ => todo!(),
    }
}

impl<'a> SkerryScanner<'a> {
    fn process_function_error(&mut self, sig: &syn::Signature) {
        let sig_hash = calculate_sig_hash(self.prefix_stack.join(""), sig);

        let (types, composites) = extract_e_errors(&sig.output).unwrap();

        // Maybe later verify the return type to return early. The proc macro already validates
        // this for us but it would still generate the error in the background even if not used.

        let raw_name = sig.ident.to_string();

        // Convert snake_case to CamelCase
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

        let composite_name = format!("{}{}Error", self.prefix_stack.join(""), camel_case_name);

        if self
            .type_definitions
            .try_insert(
                composite_name.clone(),
                TypeDefinition::composite(CompositeType {
                    file: self.file_path.to_string(),
                    hash: sig_hash,
                    types,
                    composites,
                }),
            )
            .is_err()
        {
            self.errors.push(TypeDefinitionError::new(
                DefinitionErrorCause::NameConflict {
                    name: composite_name,
                },
                self.file_path.to_string(),
                sig_hash,
            ));
        }
    }
}

impl<'a> Visit<'a> for SkerryScanner<'a> {
    fn visit_item(&mut self, i: &'a Item) {
        match i {
            Item::Enum(e) => {
                let e = e.clone();
                if e.attrs.iter().any(|a| a.path().is_ident("skerry_global")) {
                    if self.global_error_path.is_some() {
                        panic!("Global error already defined");
                    }

                    let module = self.module_stack.join("::");
                    let global_ident = e.ident.to_string();
                    let file = self.generator.get_new_cache(&self.file_path);

                    let cache_line = postcard::to_allocvec(&CacheLine::Global {
                        module: module.clone(),
                        global_ident: global_ident.clone(),
                    })
                    .unwrap();

                    file.write(&cache_line).unwrap();

                    *self.global_error_path = Some((module, global_ident));

                    for variant in e.variants {
                        let name = variant.ident.to_string();
                        let def = TypeDefinition::simple(match variant.fields {
                            syn::Fields::Named(f) => SimpleType {
                                fields: SimpleFields::Named(
                                    f.named
                                        .into_iter()
                                        .map(|f| NamedField {
                                            name: f.ident.unwrap().to_string(),
                                            ty: f.ty.to_token_stream().to_string(),
                                        })
                                        .collect(),
                                ),
                                from: None,
                            },
                            syn::Fields::Unnamed(f) => SimpleType {
                                from: if variant.attrs.iter().any(|a| a.path().is_ident("from"))
                                    && f.unnamed.len() == 1
                                {
                                    Some(
                                        f.unnamed.first().unwrap().ty.to_token_stream().to_string(),
                                    )
                                } else {
                                    None
                                },
                                fields: SimpleFields::Unnamed(
                                    f.unnamed
                                        .into_iter()
                                        .map(|f| UnnamedField {
                                            ty: f.ty.to_token_stream().to_string(),
                                        })
                                        .collect(),
                                ),
                            },
                            syn::Fields::Unit => SimpleType {
                                fields: SimpleFields::Unit,
                                from: None,
                            },
                        });
                        self.type_definitions.insert(name.clone(), def.clone());

                        let cache_line =
                            postcard::to_allocvec(&CacheLine::Definition(name, def)).unwrap();

                        file.write(&cache_line).unwrap();
                    }
                }
            }
            Item::Macro(_m) => {
                // let last_segment = m.mac.path.segments.last();
                // if last_segment.map_or(false, |s| s.ident == "skerry_include") {
                //     if self.module.is_some() {
                //         panic!("skerry_include!() called twice.");
                //     }
                //     *self.module = Some(self.module_stack.join("::"));

                //     let file = self.generator.get_new_cache(&self.file_path);
                //     let cache_line =
                //         postcard::to_allocvec(&CacheLine::Module(self.module.clone().unwrap()))
                //             .unwrap();
                //     file.write(&cache_line).unwrap();
                // } else if last_segment.map_or(false, |s| s.ident == "import_error") {
                //     // let hash = calculate_ident_hash(&ident);
                // }
                // visit::visit_item(self, i);
                // return;
            }
            _ => {
                visit::visit_item(self, i);
                return;
            }
        };

        visit::visit_item(self, i);
    }

    fn visit_item_mod(&mut self, i: &'a syn::ItemMod) {
        self.module_stack.push(i.ident.to_string());
        syn::visit::visit_item_mod(self, i);
        self.module_stack.pop();
    }

    fn visit_item_impl(&mut self, i: &'a ItemImpl) {
        if !i.attrs.iter().any(|attr| attr.path().is_ident("skerry")) {
            return;
        }

        let self_name = if let Type::Path(tp) = &*i.self_ty {
            tp.path.segments.last().map(|s| s.ident.to_string())
        } else {
            None
        };

        let prefix = self_name.unwrap_or_else(|| "Unknown".to_string());

        self.prefix_stack.push(prefix);
        visit::visit_item_impl(self, i);
        self.prefix_stack.pop();
    }

    fn visit_item_trait(&mut self, i: &'a ItemTrait) {
        if !i.attrs.iter().any(|attr| attr.path().is_ident("skerry")) {
            return;
        }

        self.prefix_stack.push(i.ident.to_string());
        visit::visit_item_trait(self, i);
        self.prefix_stack.pop();
    }

    fn visit_item_fn(&mut self, i: &'a syn::ItemFn) {
        if !i.attrs.iter().any(|attr| attr.path().is_ident("skerry")) {
            return;
        }
        self.process_function_error(&i.sig);
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'a syn::TraitItemFn) {
        self.process_function_error(&i.sig);
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'a syn::ImplItemFn) {
        self.process_function_error(&i.sig);
        syn::visit::visit_impl_item_fn(self, i);
    }
}

#[derive(Serialize, Deserialize)]
enum CacheLine {
    Global {
        module: String,
        global_ident: String,
    },
    Definition(String, TypeDefinition),
    Errors(TypeDefinitionError),
}

pub struct SkerryGenerator {
    module_override: Option<String>,

    cache_files: HashMap<String, fs::File>,
    out_dir: PathBuf,
    new_cache_dir: PathBuf,
}

pub enum SkerryCodeGenError {
    MissingGlobalDefinition,
}
impl SkerryGenerator {
    pub fn new() -> Self {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap()).join("skerry");
        let new_cache_dir = out_dir.join("new_cache");

        SkerryGenerator {
            module_override: None,
            cache_files: HashMap::new(),
            out_dir,
            new_cache_dir,
        }
    }

    /// The path to the module where `skerry_include!()` is called. This is
    /// automatically detected by the generator, only override if absolutely
    /// needed.
    pub fn override_module(mut self, module_path: impl Into<String>) -> Self {
        self.module_override = Some(module_path.into());
        self
    }

    fn touch_stamp(path: &std::path::Path) {
        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .ok();
    }

    fn needs_processing(
        file_path: &std::path::Path,
        stamp_mtime: &std::io::Result<SystemTime>,
    ) -> bool {
        let stamp_mtime = match stamp_mtime {
            Ok(mtime) => mtime,
            Err(_) => return true,
        };

        let file_mtime = match fs::metadata(file_path).and_then(|m| m.modified()) {
            Ok(mtime) => mtime,
            Err(_) => return true,
        };

        file_mtime > *stamp_mtime
    }

    fn get_new_cache(&mut self, path_str: &str) -> &mut fs::File {
        match self.cache_files.entry(path_str.to_string()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let path = self
                    .new_cache_dir
                    .join(path_str)
                    .with_added_extension("cache");

                fs::create_dir_all(path.parent().unwrap())
                    .expect("Could not create cache directory");

                let file = fs::File::create(&path).expect("Could not create cache file");

                entry.insert(file)
            }
        }
    }

    pub fn generate(mut self) -> Result<(), SkerryCodeGenError> {
        println!("cargo:rerun-if-changed=src");
        let old_cache_dir = self.out_dir.join("cache");

        fs::create_dir_all(&self.out_dir).unwrap();
        fs::create_dir_all(&old_cache_dir).unwrap();
        fs::create_dir_all(&self.new_cache_dir).unwrap();
        let _ = fs::remove_dir_all(&self.out_dir.join("expansions"));

        let stamp_path = self.out_dir.join("skerry.stamp");

        let stamp_mtime = fs::metadata(&stamp_path).and_then(|m| m.modified());

        let mut type_definitions = HashMap::new();
        let mut failures: Vec<TypeDefinitionError> = Vec::new();
        let mut expansions: HashMap<String, Vec<String>> = HashMap::new();
        let mut global_error_path: Option<(String, String)> = None;

        for entry in walkdir::WalkDir::new("src")
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let path_str = path.to_str().unwrap_or("unknown");

            if path.extension().map_or(false, |ext| ext == "rs") {
                if !Self::needs_processing(path, &stamp_mtime) {
                    if let Ok(bytes) =
                        fs::read(old_cache_dir.join(path).with_added_extension("cache"))
                    {
                        let mut bytes = bytes.as_slice();
                        let mut cache_line: CacheLine;
                        loop {
                            if bytes.len() == 0 {
                                break;
                            }

                            (cache_line, bytes) = postcard::take_from_bytes(&bytes).unwrap();
                            match cache_line {
                                CacheLine::Global {
                                    module,
                                    global_ident,
                                } => {
                                    global_error_path = Some((module, global_ident));
                                }
                                CacheLine::Definition(name, def) => {
                                    type_definitions.insert(name, def);
                                }
                                CacheLine::Errors(def) => {
                                    failures.push(def);
                                }
                            }
                        }
                        continue;
                    }
                }

                let content = fs::read_to_string(path).unwrap_or_default();

                let relative = path.strip_prefix("src").unwrap();
                let mut module_stack = vec!["crate".to_string()];
                for component in relative.parent().unwrap().components() {
                    module_stack.push(component.as_os_str().to_string_lossy().to_string());
                }
                let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
                if file_stem != "mod" && file_stem != "lib" && file_stem != "main" {
                    module_stack.push(file_stem);
                }

                let syntax_tree = match syn::parse_file(&content) {
                    Ok(tree) => tree,
                    Err(_) => continue, // Skip files with syntax errors
                };

                let mut scanner = SkerryScanner {
                    file_path: path_str,
                    type_definitions: &mut type_definitions,
                    errors: &mut failures,
                    prefix_stack: Vec::new(),
                    module_stack,
                    generator: &mut self,
                    global_error_path: &mut global_error_path,
                };

                visit::visit_file(&mut scanner, &syntax_tree);
            }
        }

        let Some((module, global_error_ident)) = global_error_path else {
            return Err(SkerryCodeGenError::MissingGlobalDefinition);
        };

        let module = self.module_override.take().unwrap_or(module);

        let mut ts = TopologicalSort::<String>::new();
        let mut writer = SkerryWriter::new(&self.out_dir, &global_error_ident, &module);

        // Validate and generate errors
        for (name, def) in &type_definitions {
            {
                let file = self.get_new_cache(match &def.ty {
                    TypeDefinitionType::Simple(ty) => {
                        writer.add_marker(name).unwrap();
                        if let Some(from) = &ty.from {
                            writer.add_from(from, name).unwrap();
                        }
                        continue;
                    }
                    TypeDefinitionType::Composite(composite_type) => &composite_type.file,
                });

                let cache_line =
                    postcard::to_allocvec(&CacheLine::Definition(name.clone(), def.clone()))
                        .unwrap();
                file.write(&cache_line).unwrap();
            }

            let TypeDefinition { ty } = def;

            match ty {
                TypeDefinitionType::Simple(_) => {
                    unreachable!()
                }
                TypeDefinitionType::Composite(CompositeType {
                    types,
                    composites,
                    file,
                    hash,
                }) => {
                    let mut missing_errors = vec![];
                    let mut remove_asterisk = vec![];
                    let mut add_asterisk = vec![];

                    // Checking for errors
                    for plain_type in types {
                        if let Some(t) = type_definitions.get(plain_type) {
                            if let TypeDefinitionType::Composite { .. } = t.ty {
                                add_asterisk.push(plain_type.clone());
                            }
                        } else {
                            missing_errors.push(plain_type.clone());
                        }
                    }

                    for composite in composites {
                        if let Some(t) = type_definitions.get(composite) {
                            if let TypeDefinitionType::Simple { .. } = t.ty {
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
                        failures.push(TypeDefinitionError::new(
                            DefinitionErrorCause::WrongErrorExpansion {
                                missing_errors,
                                remove_asterisk,
                                add_asterisk,
                            },
                            file.clone(),
                            *hash,
                        ));
                        continue;
                    }

                    // Add the node to the sorter
                    ts.insert(name.clone());

                    // For every composite this error depends on, add a dependency link
                    for dependency in composites {
                        ts.add_dependency(dependency.clone(), name.clone());
                    }

                    writer
                        .add_result(*hash, WrittenResult::Ok(format!("{module}::{name}")))
                        .unwrap();
                }
            }
        }

        let mut sorted_order = Vec::new();

        while let Some(name) = ts.pop() {
            sorted_order.push(name);
        }

        // Cycle detected
        if !ts.is_empty() {
            // TODO: Return a better error, probably by expanding the macro at
            // the e![] locations
            panic!("Circular dependency detected in error definitions!");
        }

        for name in sorted_order.into_iter() {
            if let Some(TypeDefinitionType::Composite(CompositeType {
                types, composites, ..
            })) = type_definitions.get(&name).and_then(|t| Some(&t.ty))
            {
                let mut all_types: Vec<String> = types.clone();

                for composite in composites {
                    let expansion = match expansions.get(composite) {
                        Some(v) => v,
                        None => continue,
                    };
                    all_types.extend_from_slice(expansion);
                }
                // TODO: This entire section is horrible, fix this shit later
                all_types.sort();
                all_types.dedup();

                writer.add_not(&name).unwrap();
                let types = all_types
                    .iter()
                    .map(|t| match &type_definitions.get(t).unwrap().ty {
                        TypeDefinitionType::Simple(simple_type) => (t.as_str(), simple_type),
                        TypeDefinitionType::Composite(_) => unreachable!(),
                    })
                    .collect::<Vec<_>>();
                writer.add_define(&name, &types).unwrap();

                expansions.insert(name, all_types);
            }
        }

        for error in failures {
            {
                let file = self.get_new_cache(&error.file);

                let cache_line = postcard::to_allocvec(&CacheLine::Errors(error.clone())).unwrap();

                file.write(&cache_line).unwrap();
            }

            writer
                .add_result(error.hash, WrittenResult::RawError { msg: error.msg })
                .unwrap();
        }

        writer.finish().unwrap();
        fs::remove_dir_all(&old_cache_dir).unwrap();
        fs::rename(self.new_cache_dir, old_cache_dir).unwrap();
        Self::touch_stamp(&stamp_path);
        Ok(())
    }
}
