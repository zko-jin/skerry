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

use hashbrown::{
    HashMap,
    hash_map::Entry,
};
use serde::{
    Deserialize,
    Serialize,
};
use syn::{
    GenericArgument,
    Item,
    ItemImpl,
    ItemTrait,
    PathArguments,
    Type,
    spanned::Spanned,
    visit::{
        self,
        Visit,
    },
};
use topological_sort::TopologicalSort;

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
    line: usize,
    ty: TypeDefinitionType,
}

impl TypeDefinition {
    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn simple(file: String, line: usize, mod_path: String) -> Self {
        Self {
            file,
            line,
            ty: TypeDefinitionType::Simple { mod_path },
        }
    }

    pub fn composite(file: String, line: usize, composite: CompositeType) -> Self {
        Self {
            file,
            line,
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
    NotInResult,
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
            DefinitionErrorCause::NotInResult => "e![] can only be used inside Result".to_string(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TypeDefinitionError {
    msg: String,
    file: String,
    line: usize,
}

impl TypeDefinitionError {
    pub fn new(cause: DefinitionErrorCause, file: String, line: usize) -> Self {
        Self {
            msg: cause.to_msg(),
            file,
            line,
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
    fn process_function_error(&mut self, ident: &syn::Ident, output: &'a syn::ReturnType) {
        if let syn::ReturnType::Type(_, ty) = output {
            if let Some((types, composites)) = extract_skerry_macro_types(ty) {
                let raw_name = ident.to_string();

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

                let composite_name =
                    format!("{}{}Error", self.prefix_stack.join(""), camel_case_name);

                if self
                    .type_definitions
                    .try_insert(
                        composite_name.clone(),
                        TypeDefinition::composite(
                            self.file_path.to_string(),
                            ty.span().start().line,
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
                        ty.span().start().line,
                    ));
                }
            } else {
                // If it's a function but doesn't have the skerry macro in Result
                self.errors.push(TypeDefinitionError {
                    msg: DefinitionErrorCause::NotInResult.to_msg(),
                    file: self.file_path.to_string(),
                    line: ty.span().start().line,
                });
            }
        }
    }
}

impl<'a> Visit<'a> for SkerryScanner<'a> {
    fn visit_item(&mut self, i: &'a Item) {
        let attrs = match i {
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

        let (attrs, ident) = attrs;
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
                    TypeDefinition::simple(
                        self.file_path.to_string(),
                        attr.span().start().line,
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
                    attr.span().start().line,
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
        self.process_function_error(&i.sig.ident, &i.sig.output);
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'a syn::TraitItemFn) {
        self.process_function_error(&i.sig.ident, &i.sig.output);
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'a syn::ImplItemFn) {
        self.process_function_error(&i.sig.ident, &i.sig.output);
        syn::visit::visit_impl_item_fn(self, i);
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
        // writeln!(
        //     self.writer,
        //     "impl skerry::skerry_internals::Contains<{module}::{ty}> for {module}::{ty}{{}}"
        // )?;
        // write!(
        //     self.writer,
        //     "impl<T: skerry::skerry_internals::Contains<{module}::{ty}>>\
        //     skerry::skerry_internals::IsSupersetOf<T> for {module}::{ty}{{}}
        //     impl From<{module}::{ty}> for GlobalErrors<{module}::{ty}> {{
        //         fn from(val: {module}::{ty}) -> Self {{
        //             GlobalErrors::{ty}(val)
        //         }}
        //     }}"
        // )?;

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

    pub fn add_macro_arm_empty(&mut self, file: &str, line: usize) -> io::Result<()> {
        write!(self.macro_arms, "({file:?}, {line}) => {{}};",)
    }

    pub fn add_macro_arm_composite(
        &mut self,
        file: &str,
        line: usize,
        module: &str,
        ty: &str,
    ) -> io::Result<()> {
        write!(self.macro_arms, "({file:?}, {line}) => {{{module}::{ty}}};",)
    }

    pub fn add_macro_arm_error(&mut self, error: &TypeDefinitionError) -> io::Result<()> {
        write!(
            self.macro_arms,
            "({file:?}, {line}) => {{compile_error!(\"{file}:{line} - {msg}\")}};",
            file = error.file,
            line = error.line,
            msg = error.msg
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
        // write!(
        //     writer,
        //     "($file:expr, $line:expr) => {{ compile_error!(concat!\
        //     (\"Skerry Sync Error: No macro generated for \", $file, \":\", $line));}}}}"
        // )?;
        write!(writer, "($file:expr, $line:expr) => {{}}}}")?;

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

                if !content.contains("e![")
                    && !content.contains("#[skerry_error]")
                    && !content.contains("skerry_include!")
                {
                    continue;
                }

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

            let TypeDefinition { file, line, ty } = def;

            match ty {
                TypeDefinitionType::Simple { mod_path } => {
                    writer.add_macro_arm_empty(file, *line).unwrap();
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
                            *line,
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
                        .add_macro_arm_composite(file, *line, &module, name)
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

            writer.add_macro_arm_error(&error).unwrap();
        }

        writer.finish().unwrap();
        fs::remove_dir_all(&old_cache_dir).unwrap();
        fs::rename(self.new_cache_dir, old_cache_dir).unwrap();
        Self::touch_stamp(&stamp_path);
        Ok(())
    }
}
