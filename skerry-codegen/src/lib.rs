use std::{
    env,
    fs::{
        self,
        File,
    },
    io::{
        self,
        BufWriter,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    time::SystemTime,
};

use ahash::RandomState;
use hashbrown::{
    HashMap,
    hash_map::Entry,
};
use quote::ToTokens;
use serde::{
    Deserialize,
    Serialize,
};
use syn::{
    Item,
    ItemImpl,
    ItemTrait,
    Type,
    visit::{
        self,
        Visit,
    },
};
use topological_sort::TopologicalSort;

pub fn calculate_ident_hash(ident: &syn::Ident) -> u64 {
    let hasher = RandomState::with_seeds(0, 0, 0, 0);
    hasher.hash_one(ident.to_string())
}

pub fn calculate_sig_hash(sig: &syn::Signature) -> u64 {
    let sig_string = sig.to_token_stream().to_string();
    let normalized: String = sig_string.chars().filter(|c| !c.is_whitespace()).collect();

    let hasher = RandomState::with_seeds(0, 0, 0, 0);
    hasher.hash_one(normalized)
}

#[derive(Clone, Serialize, Deserialize)]
struct CompositeType {
    types: Vec<String>,
    composites: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
enum TypeDefinitionType {
    Simple { mod_path: String },
    Composite(CompositeType),
}

#[derive(Clone, Serialize, Deserialize)]
struct TypeDefinition {
    file: String,
    hash: u64,
    ty: TypeDefinitionType,
}

impl TypeDefinition {
    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn simple(file: String, hash: u64, mod_path: String) -> Self {
        Self {
            file,
            hash,
            ty: TypeDefinitionType::Simple { mod_path },
        }
    }

    pub fn composite(file: String, hash: u64, composite: CompositeType) -> Self {
        Self {
            file,
            hash,
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
    module: &'a mut Option<String>,
    generator: &'a mut SkerryGenerator,
}

impl<'a> SkerryScanner<'a> {
    fn process_function_error(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature) {
        let sig_hash = calculate_sig_hash(sig);

        let mut types = Vec::new();
        let mut composites = Vec::new();

        let Some(attr) = attrs.iter().find(|a| a.path().is_ident("e")) else {
            println!("cargo::warning=not stuff {}", sig.ident);
            return;
        };

        if let Ok(list) = attr.parse_args_with(|input: syn::parse::ParseStream| {
            let mut errors = Vec::new();
            while !input.is_empty() {
                let is_composite = input.peek(syn::Token![*]);
                if is_composite {
                    input.parse::<syn::Token![*]>()?;
                }
                let id: syn::Ident = input.parse()?;
                errors.push((id.to_string(), is_composite));
                if input.peek(syn::Token![,]) {
                    input.parse::<syn::Token![,]>()?;
                }
            }
            Ok(errors)
        }) {
            for (name, is_composite) in list {
                if is_composite {
                    composites.push(name);
                } else {
                    types.push(name);
                }
            }
        } else {
            // No need to handle Err, the proc macro already validated the syntax
            return;
        }

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
                TypeDefinition::composite(
                    self.file_path.to_string(),
                    sig_hash,
                    CompositeType { types, composites },
                ),
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
        let (attrs, ident) = match i {
            Item::Struct(s) => {
                let ident = &s.ident;
                let mut s = s.clone();
                let attrs = std::mem::replace(&mut s.attrs, vec![]);
                (attrs, ident)
            }
            Item::Enum(e) => {
                let ident = &e.ident;
                let mut e = e.clone();
                let attrs = std::mem::replace(&mut e.attrs, vec![]);
                (attrs, ident)
            }
            Item::Macro(m) => {
                if m.mac
                    .path
                    .segments
                    .last()
                    .map_or(false, |s| s.ident == "skerry_include")
                {
                    if self.module.is_some() {
                        panic!("skerry_include!() called twice.");
                    }
                    *self.module = Some(self.module_stack.join("::"));

                    let file = self.generator.get_new_cache(&self.file_path);
                    let cache_line =
                        postcard::to_allocvec(&CacheLine::Module(self.module.clone().unwrap()))
                            .unwrap();
                    file.write(&cache_line).unwrap();
                }
                visit::visit_item(self, i);
                return;
            }
            _ => {
                visit::visit_item(self, i);
                return;
            }
        };

        let hash = calculate_ident_hash(&ident);

        if attrs.iter().any(|attr| {
            if attr.path().is_ident("skerry_error") {
                true
            } else {
                false
            }
        }) {
            if self
                .type_definitions
                .try_insert(
                    ident.to_string(),
                    TypeDefinition::simple(
                        self.file_path.to_string(),
                        hash,
                        self.module_stack.join("::"),
                    ),
                )
                .is_err()
            {
                self.errors.push(TypeDefinitionError::new(
                    DefinitionErrorCause::NameConflict {
                        name: ident.to_string(),
                    },
                    self.file_path.to_string(),
                    hash,
                ));
            }
        }

        visit::visit_item(self, i);
    }

    fn visit_item_mod(&mut self, i: &'a syn::ItemMod) {
        self.module_stack.push(i.ident.to_string());
        syn::visit::visit_item_mod(self, i);
        self.module_stack.pop();
    }

    fn visit_item_impl(&mut self, i: &'a ItemImpl) {
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
        let prefix = i.ident.to_string();

        self.prefix_stack.push(prefix);
        visit::visit_item_trait(self, i);
        self.prefix_stack.pop();
    }

    fn visit_item_fn(&mut self, i: &'a syn::ItemFn) {
        self.process_function_error(&i.attrs, &i.sig);
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'a syn::TraitItemFn) {
        self.process_function_error(&i.attrs, &i.sig);
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'a syn::ImplItemFn) {
        self.process_function_error(&i.attrs, &i.sig);
        syn::visit::visit_impl_item_fn(self, i);
    }
}

#[derive(Serialize, Deserialize)]
enum CacheLine {
    Module(String),
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
    MissingInclude,
}

struct SkerryWriter {
    writer: BufWriter<File>,
    global_variants: BufWriter<Vec<u8>>,
    privates: BufWriter<Vec<u8>>,
    macro_arms: BufWriter<Vec<u8>>,
}

impl SkerryWriter {
    pub fn new(path: &Path) -> Self {
        let file = File::create(path.join("skerry_gen.rs")).unwrap();
        Self {
            writer: BufWriter::new(file),
            global_variants: BufWriter::new(Vec::new()),
            privates: BufWriter::new(Vec::new()),
            macro_arms: BufWriter::new(Vec::new()),
        }
    }

    pub fn add_variant(&mut self, module: &str, ty: &str) -> io::Result<()> {
        write!(self.global_variants, "{ty}({module}::{ty}),")?;
        Ok(())
    }

    pub fn add_define(&mut self, ty: &str, variants: &Vec<String>) -> io::Result<()> {
        write!(self.writer, "pub enum {ty} {{")?;
        for variant in variants {
            write!(self.writer, "{variant}(crate::{variant}),")?;
        }
        write!(self.writer, "}}")?;

        for variant in variants {
            write!(
                self.writer,
                "impl skerry::skerry_internals::Contains<crate::{variant}> for {ty}{{}}"
            )?;
        }
        write!(self.writer, "impl <T:")?;
        for (i, t) in variants.iter().enumerate() {
            if i > 0 {
                write!(self.writer, "+")?;
            }
            write!(
                self.writer,
                "skerry::skerry_internals::Contains<crate::{t}>",
            )?;
        }
        write!(
            self.writer,
            "> skerry::skerry_internals::IsSubsetOf<T> for {ty}{{}}"
        )?;

        write!(
            self.writer,
            "impl<E: Into<GlobalErrors> + skerry::skerry_internals::IsSubsetOf<{ty}> + \
            __skerry_private::Not{ty}> From<E> for {ty} {{fn from(val:E)->{ty}{{match val.into(){{"
        )?;
        for t in variants {
            writeln!(self.writer, "GlobalErrors::{t}(v) => {ty}::{t}(v),",)?;
        }
        write!(self.writer, "_ => unreachable!()}}}}}}")?;

        for t in variants {
            writeln!(
                self.writer,
                "impl From<crate::{t}> for {ty} {{
                    fn from(val: crate::{t}) -> {ty} {{
                        {ty}::{t}(val)
                    }}
                }}",
            )?;
        }

        writeln!(
            self.writer,
            "impl From<{ty}> for GlobalErrors {{
                fn from(val: {ty}) -> GlobalErrors {{
                    match val {{",
        )?;
        for t in variants {
            writeln!(self.writer, "{ty}::{t}(v) => GlobalErrors::{t}(v),",)?;
        }
        writeln!(self.writer, "}}}}}}")?;
        Ok(())
    }

    pub fn add_macro_arm_empty(&mut self, hash: u64) -> io::Result<()> {
        write!(self.macro_arms, "({hash}) => {{}};",)
    }

    pub fn add_macro_arm_composite(&mut self, hash: u64, module: &str, ty: &str) -> io::Result<()> {
        write!(self.macro_arms, "({hash}) => {{{module}::{ty}}};",)
    }

    pub fn add_macro_arm_error(&mut self, hash: u64, msg: &str) -> io::Result<()> {
        write!(
            self.macro_arms,
            "({hash}) => {{compile_error!(\"{msg}\")}};",
        )
    }

    pub fn add_not(&mut self, ty: &str) -> io::Result<()> {
        write!(
            self.privates,
            "pub auto trait Not{ty} {{}} impl !Not{ty} for super::{ty} {{}}"
        )
    }

    pub fn finish(self) -> io::Result<()> {
        let SkerryWriter {
            mut writer,
            global_variants,
            privates,
            macro_arms,
        } = self;

        write!(writer, "pub enum GlobalErrors{{")?;
        writer.write(&global_variants.into_inner()?)?;
        write!(writer, "}}")?;

        write!(writer, "mod __skerry_private{{")?;
        writer.write(&privates.into_inner()?)?;
        write!(writer, "}}")?;

        write!(writer, "#[macro_export]\nmacro_rules! skerry_invoke {{")?;
        writer.write(&macro_arms.into_inner()?)?;
        write!(
            writer,
            "($file:expr) => {{ compile_error!(concat!\
            (\"Skerry Sync Error: Code not yet generated for this call\"))}}}}"
        )?;

        // Is this needed? Maybe dropping the writer flushes it already
        writer.flush()
    }
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

        let stamp_path = self.out_dir.join("skerry.stamp");

        let stamp_mtime = fs::metadata(&stamp_path).and_then(|m| m.modified());

        let mut type_definitions = HashMap::new();
        let mut failures: Vec<TypeDefinitionError> = Vec::new();
        let mut module = None;
        let mut expansions: HashMap<String, Vec<String>> = HashMap::new();

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
                                CacheLine::Module(s) => {
                                    module = Some(s);
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
                    module: &mut module,
                    generator: &mut self,
                };

                visit::visit_file(&mut scanner, &syntax_tree);
            }
        }

        let Some(module) = module else {
            return Err(SkerryCodeGenError::MissingInclude);
            // panic!("skerry_include!(); never called!");
        };

        let module = self.module_override.take().unwrap_or(module);

        let mut ts = TopologicalSort::<String>::new();
        let mut writer = SkerryWriter::new(&self.out_dir);

        // Validate and generate errors
        for (name, def) in &type_definitions {
            {
                let file = self.get_new_cache(def.file());

                let cache_line =
                    postcard::to_allocvec(&CacheLine::Definition(name.clone(), def.clone()))
                        .unwrap();
                file.write(&cache_line).unwrap();
            }

            let TypeDefinition { file, hash, ty } = def;

            match ty {
                TypeDefinitionType::Simple { mod_path } => {
                    writer.add_macro_arm_empty(*hash).unwrap();
                    writer.add_variant(mod_path, name).unwrap();
                }
                TypeDefinitionType::Composite(CompositeType { types, composites }) => {
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

                    writer.add_not(name).unwrap();

                    // Add the node to the sorter
                    ts.insert(name.clone());

                    // For every composite this error depends on, add a dependency link
                    for dependency in composites {
                        ts.add_dependency(dependency.clone(), name.clone());
                    }

                    writer
                        .add_macro_arm_composite(*hash, &module, name)
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
                    all_types.extend(expansions.get(composite).unwrap().clone());
                }
                // TODO: This entire section is horrible, fix this shit later
                all_types.sort();
                all_types.dedup();

                writer.add_define(&name, &all_types).unwrap();

                expansions.insert(name, all_types);
            }
        }

        for error in failures {
            {
                let file = self.get_new_cache(&error.file);

                let cache_line = postcard::to_allocvec(&CacheLine::Errors(error.clone())).unwrap();

                file.write(&cache_line).unwrap();
            }

            writer.add_macro_arm_error(error.hash, &error.msg).unwrap();
        }

        writer.finish().unwrap();
        fs::remove_dir_all(&old_cache_dir).unwrap();
        fs::rename(self.new_cache_dir, old_cache_dir).unwrap();
        Self::touch_stamp(&stamp_path);
        Ok(())
    }
}
