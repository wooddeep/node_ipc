extern crate napi_build;
extern crate cc;
use cc::Build;

fn main() {
  napi_build::setup();

  // Build::new()
  //     .file("src/ipc.c")
  //     .compile("ipc");
}
