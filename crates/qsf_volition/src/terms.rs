/// Normalize input text into lowercase word tokens, deduplicated, in order of first
/// appearance. Used by goal selection and candidate proposal matching.
pub fn normalize_terms(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            if !terms.iter().any(|term| term == &current) {
                terms.push(current.clone());
            }
            current.clear();
        }
    }

    if !current.is_empty() && !terms.iter().any(|term| term == &current) {
        terms.push(current);
    }

    terms
}
