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

pub struct SkerryWriter<'a> {
    writer: BufWriter<File>,
    global_variants: BufWriter<Vec<u8>>,
    privates: BufWriter<Vec<u8>>,
    expand_folder: PathBuf,
    global_error_ident: &'a str,
}

impl<'a> SkerryWriter<'a> {
    pub fn new(path: &Path, global_error_ident: &'a str) -> Self {
        let expand_folder = path.join("expansions/");
        std::fs::create_dir_all(&expand_folder).unwrap();
        let file = File::create(path.join("skerry_gen.rs")).unwrap();
        Self {
            writer: BufWriter::new(file),
            global_variants: BufWriter::new(Vec::new()),
            privates: BufWriter::new(Vec::new()),
            expand_folder,
            global_error_ident,
        }
    }

    pub fn add_variant(&mut self, module: &str, ty: &str) -> io::Result<()> {
        write!(self.global_variants, "{ty}({module}::{ty}),")?;
        Ok(())
    }

    pub fn add_define(&mut self, ty: &str, variants: &Vec<String>) -> io::Result<()> {
        let global_error = self.global_error_ident;
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
            "impl<E: Into<{global_error}> + skerry::skerry_internals::IsSubsetOf<{ty}> + \
            __skerry_private::Not{ty}> From<E> for {ty} {{fn from(val:E)->{ty}{{match val.into(){{"
        )?;
        for t in variants {
            writeln!(self.writer, "{global_error}::{t}(v) => {ty}::{t}(v),",)?;
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
            "impl From<{ty}> for {global_error} {{
                fn from(val: {ty}) -> {global_error} {{
                    match val {{",
        )?;
        for t in variants {
            writeln!(self.writer, "{ty}::{t}(v) => {global_error}::{t}(v),")?;
        }
        writeln!(self.writer, "}}}}}}")?;
        Ok(())
    }

    pub fn add_macro_arm_empty(&mut self, hash: u64) -> io::Result<()> {
        std::fs::write(self.expand_folder.join(hash.to_string()), "+")
    }

    pub fn add_macro_arm_composite(&mut self, hash: u64, module: &str, ty: &str) -> io::Result<()> {
        std::fs::write(
            self.expand_folder.join(hash.to_string()),
            &format!("+{module}::{ty}"),
        )
    }

    pub fn add_macro_arm_error(&mut self, hash: u64, msg: &str) -> io::Result<()> {
        std::fs::write(
            self.expand_folder.join(hash.to_string()),
            &format!("!{msg}"),
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
            global_error_ident,
            ..
        } = self;

        write!(writer, "pub enum {global_error_ident}{{",)?;
        writer.write(&global_variants.into_inner()?)?;
        write!(writer, "}}")?;

        write!(writer, "mod __skerry_private{{")?;
        writer.write(&privates.into_inner()?)?;
        write!(writer, "}}")?;

        // Is this needed? Maybe dropping the writer flushes it already
        writer.flush()
    }
}
