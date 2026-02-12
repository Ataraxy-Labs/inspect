use inspect_core::types::ReviewResult;

pub fn print(result: &ReviewResult) {
    let json = serde_json::to_string_pretty(result).expect("failed to serialize");
    println!("{}", json);
}
