use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn string_to_unique_integer(s: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish() as u32
}

fn test_str_to_int() -> i32 {
    let s = "Hello, World!";
    let unique_integer = string_to_unique_integer(s);
    println!("String: {}", s);
    println!("Unique Integer: {}", unique_integer);
    0
}

// 测试代码
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_x() {
    let out = test_str_to_int();
    assert_eq!(out, 0);
  }
}
