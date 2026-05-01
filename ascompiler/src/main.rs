use std::env;

fn main() {
    ascompiler::cli::run(env::args().collect());
}
