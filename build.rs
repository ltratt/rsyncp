use std::{
    env::var,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn get_c_const(out_dir: &Path, name: &str, includes: &str, ctype: &str) -> String {
    let c_path = out_dir.join("probe.c");
    let printf_type = match ctype {
        "int" => "%d",
        "unsigned long" => "%lu",
        "short" => "%hu",
        _ => todo!(),
    };
    fs::write(
        &c_path,
        format!(
            r#"
#include <stdio.h>
{includes}

int main(void) {{
  printf("{printf_type}", ({ctype}) {name});
  return 0;
}}
"#
        ),
    )
    .unwrap();

    let exe_path = out_dir.join("probe");
    let mut cc = Command::new(var("CC").unwrap_or_else(|_| "cc".into()));
    cc.arg(&c_path).arg("-o").arg(&exe_path);
    let _ = run_cmd(cc);

    run_cmd(Command::new(&exe_path))
}

fn run_cmd(mut cmd: Command) -> String {
    let c = cmd.output().unwrap_or_else(|e| panic!("{e}"));
    if !c.status.success() {
        panic!("{}", String::from_utf8_lossy(&c.stderr));
    }
    String::from_utf8(c.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(var("OUT_DIR").unwrap());

    let f_getfl = get_c_const(&out_dir, "F_GETFL", "#include <fcntl.h>", "int");
    let f_setfl = get_c_const(&out_dir, "F_SETFL", "#include <fcntl.h>", "int");
    let o_nonblock = get_c_const(&out_dir, "O_NONBLOCK", "#include <fcntl.h>", "int");
    let pollhup = get_c_const(&out_dir, "POLLHUP", "#include <poll.h>", "short");
    let pollin = get_c_const(&out_dir, "POLLIN", "#include <poll.h>", "short");
    let sigint = get_c_const(&out_dir, "SIGINT", "#include <signal.h>", "int");
    let sigwinch = get_c_const(&out_dir, "SIGWINCH", "#include <signal.h>", "int");
    let tiocgwinsz = get_c_const(
        &out_dir,
        "TIOCGWINSZ",
        "#include <sys/ioctl.h>",
        "unsigned long",
    );

    fs::write(
        out_dir.join("plat.rs"),
        format!(
            "use std::ffi::{{c_int, c_short, c_ulong}};\n\
             pub const F_GETFL: c_int = {f_getfl};\n\
             pub const F_SETFL: c_int = {f_setfl};\n\
             pub const O_NONBLOCK: c_int = {o_nonblock};\n\
             pub const POLLHUP: c_short = {pollhup};\n\
             pub const POLLIN: c_short = {pollin};\n\
             pub const SIGINT: c_int = {sigint};\n\
             pub const SIGWINCH: c_int = {sigwinch};\n\
             pub const TIOCGWINSZ: c_ulong = {tiocgwinsz};\n",
        ),
    )
    .unwrap();
}
