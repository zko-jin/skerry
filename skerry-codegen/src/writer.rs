use std::{
    fs::File,
    io::{
        self,
        BufWriter,
        Write as _,
    },
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::SimpleType;

#[derive(Serialize, Deserialize)]
pub enum EnumErrorLocation {
    Variant(String),
}

#[derive(Serialize, Deserialize)]
pub enum WrittenResult {
    Ok(String),
    EnumError {
        location: EnumErrorLocation,
        msg: String,
    },
    RawError {
        msg: String,
    },
}

pub struct SkerryWriter<'a> {
    writer: BufWriter<File>,
    privates: BufWriter<Vec<u8>>,
    expand_folder: PathBuf,
    global_error_path: String,
    global_module: &'a str,
}

impl<'a> SkerryWriter<'a> {
    pub fn new(path: &Path, global_error_ident: &'a str, global_module: &'a str) -> Self {
        let expand_folder = path.join("expansions/");
        std::fs::create_dir_all(&expand_folder).unwrap();
        let file = File::create(path.join("skerry_gen.rs")).unwrap();
        Self {
            writer: BufWriter::new(file),
            privates: BufWriter::new(Vec::new()),
            expand_folder,
            global_error_path: format!("{}::{}", global_module, global_error_ident),
            global_module,
        }
    }

    // pub fn add_variant(&mut self, ty: &str) -> io::Result<()> {
    //     // write!(self.global_variants, "{ty}({module}::{ty}),")?;
    //     Ok(())
    // }

    pub fn add_define(&mut self, ty: &str, variants: &Vec<(&str, &SimpleType)>) -> io::Result<()> {
        let global_error = &self.global_error_path;

        write!(self.writer, "pub enum {ty}{{")?;
        for (name, ty) in variants {
            write!(self.writer, "{name}{},", ty.fields.display_def())?;
        }
        write!(self.writer, "}}")?;

        for (name, _) in variants {
            write!(
                self.writer,
                "impl skerry::skerry_internals::Contains<__skerry_private::{name}Marker> for {ty}{{}}"
            )?;
        }
        write!(self.writer, "impl <T:")?;
        for (i, (name, _)) in variants.iter().enumerate() {
            if i > 0 {
                write!(self.writer, "+")?;
            }
            write!(
                self.writer,
                "skerry::skerry_internals::Contains<__skerry_private::{name}Marker>",
            )?;
        }
        write!(
            self.writer,
            "> skerry::skerry_internals::IsSubsetOf<T> for {ty}{{}}"
        )?;

        write!(
            self.writer,
            "impl<E: Into<{global_error}> + skerry::skerry_internals::IsSubsetOf<{ty}> + \
            __skerry_private::Not{ty}> From<E> for {ty} {{fn from(val:E)->{ty}{{match val.into(){{"
        )?;
        for (name, def) in variants {
            let expand = def.fields.display_expansion();
            writeln!(
                self.writer,
                "{global_error}::{name}{expand} => {ty}::{name}{expand},",
            )?;
        }
        write!(self.writer, "_ => unreachable!()}}}}}}")?;

        writeln!(
            self.writer,
            "impl From<{ty}> for {global_error} {{
                fn from(val: {ty}) -> {global_error} {{
                    match val {{",
        )?;
        for (name, def) in variants {
            let expand = def.fields.display_expansion();
            writeln!(
                self.writer,
                "{ty}::{name}{expand} => {global_error}::{name}{expand},"
            )?;
        }
        writeln!(self.writer, "}}}}}}")?;
        Ok(())
    }

    pub fn add_result(&mut self, hash: u64, res: WrittenResult) -> io::Result<()> {
        let bytes = postcard::to_allocvec(&res).unwrap();
        std::fs::write(self.expand_folder.join(hash.to_string()), &bytes)
    }

    pub fn add_marker(&mut self, ty: &str) -> io::Result<()> {
        writeln!(self.privates, "pub struct {ty}Marker{{}}")
    }

    pub fn add_not(&mut self, ty: &str) -> io::Result<()> {
        writeln!(
            self.privates,
            "pub auto trait Not{ty} {{}} impl !Not{ty} for super::{ty} {{}}"
        )
    }

    pub fn finish(self) -> io::Result<()> {
        let SkerryWriter {
            mut writer,
            privates,
            global_error_path,
            ..
        } = self;

        let bytes = postcard::to_allocvec(&WrittenResult::Ok(String::new())).unwrap();
        std::fs::write(self.expand_folder.join("global"), &bytes)?;

        writeln!(writer, "#[allow(unused)]\nmod __skerry_private{{")?;
        writer.write(&privates.into_inner()?)?;
        write!(writer, "}}")?;

        // Is this needed? Maybe dropping the writer flushes it already
        writer.flush()
    }
}
