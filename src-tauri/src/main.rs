// Windows GUI subsystem: no console window. Must apply in debug too — Jump List /
// “Open New Window” launches this exe directly, and a console-subsystem debug
// build would flash a black terminal each time.
#![windows_subsystem = "windows"]

fn main() {
    supermanga_lib::run()
}
