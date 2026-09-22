use std::{env, path::PathBuf};
use syntect::parsing::SyntaxDefinition;
fn main() {
    build_syntax_set();
}

/// Merge the bundled PowerShell grammar into two-face's dump once per compile
/// so runtime highlighting loads a single set.
fn build_syntax_set() {
    println!("cargo:rerun-if-changed=src/tui/powershell.sublime-syntax");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("syntaxes-newlines.bin");
    let mut builder = two_face::syntax::extra_newlines().into_builder();
    let powershell = SyntaxDefinition::load_from_str(
        include_str!("src/tui/powershell.sublime-syntax"),
        /*lines_include_newline*/ true,
        None,
    )
    .expect("bundled PowerShell syntax must be valid");
    builder.add(powershell);
    let set = builder.build();
    syntect::dumps::dump_to_uncompressed_file(&set, &out)
        .unwrap_or_else(|error| panic!("failed to dump syntax set: {error}"));
}
