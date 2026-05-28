//! Apple Books Exporter - LLM Prompt Templates

/// 构建 LLM 提示词
pub fn build_enrich_prompt(highlight: &str, context: Option<&str>) -> String {
    let mut prompt = String::new();

    // 系统角色
    prompt.push_str("你是一个阅读助手，帮助读者理解书籍内容。");
    prompt.push_str("请对以下摘录进行分析，提供：\n");
    prompt.push_str("1. 解释：用通俗易懂的语言解释这段内容的含义\n");
    prompt.push_str("2. 标签：3-5 个关键词标签，用逗号分隔\n");
    prompt.push_str("3. 复习问题：一个能帮助读者回顾和理解的问题\n\n");

    // 内容
    prompt.push_str("请按照以下 JSON 格式返回结果：\n");
    prompt.push_str("{\n");
    prompt.push_str("  \"explanation\": \"解释内容\",\n");
    prompt.push_str("  \"tags\": [\"标签 1\", \"标签 2\", \"标签 3\"],\n");
    prompt.push_str("  \"question\": \"复习问题\"\n");
    prompt.push_str("}\n\n");

    // 摘录内容
    prompt.push_str("摘录内容：\n");
    prompt.push_str("> ");
    prompt.push_str(highlight);
    prompt.push_str("\n");

    if let Some(ctx) = context {
        prompt.push_str("\n上下文：\n");
        prompt.push_str(ctx);
    }

    prompt
}

/// 构建批量处理的提示词
pub fn build_batch_prompt(highlights: &[&str]) -> String {
    let mut prompt = String::new();

    prompt.push_str("你是一个阅读助手。请分析以下摘录，为每一条提供解释、标签和复习问题。\n\n");
    prompt.push_str("请按照以下 JSON 数组格式返回结果：\n");
    prompt.push_str("[\n");
    prompt.push_str("  {\"explanation\": \"...\", \"tags\": [\"...\"], \"question\": \"...\"},\n");
    prompt.push_str("  ...\n");
    prompt.push_str("]\n\n");

    for (i, h) in highlights.iter().enumerate() {
        prompt.push_str(&format!("摘录 {}: {}\n", i + 1, h));
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_enrich_prompt() {
        let prompt = build_enrich_prompt("简短模式（short variable declaration）有些限制：", None);
        assert!(prompt.contains("解释"));
        assert!(prompt.contains("标签"));
        assert!(prompt.contains("复习问题"));
        assert!(prompt.contains("简短模式"));
    }
}