#[cfg(test)]
pub(crate) fn note(path: &str, content: &str) -> crate::parser::Note {
    crate::parser::parse(std::path::Path::new(path), content)
}
