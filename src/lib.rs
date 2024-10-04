use anyhow::Result;

mod document;
mod errors;
mod parser;

pub fn lint(input: &str) -> Result<()> {
    let node = parser::parse(input)?;
    Ok(())
}

#[cfg(test)]
use ctor::ctor;

#[cfg(test)]
#[ctor]
fn init_test_logger() {
    env_logger::builder().is_test(true).try_init().unwrap();
}
