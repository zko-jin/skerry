use skerry_codegen::SkerryGenerator;

fn main() {
    let _ = SkerryGenerator::new().global_error_ident("Test").generate();
}
