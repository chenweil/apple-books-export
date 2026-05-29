//! Apple Books Exporter - LLM Prompt Templates

/// 默认 prompt 模板
const DEFAULT_ENRICH_PROMPT: &str = "你是一个阅读助手，帮助读者理解书籍内容。请对以下摘录进行分析，提供：\n1. 解释：用通俗易懂的语言解释这段内容的含义\n2. 标签：3-5 个关键词标签，用逗号分隔\n3. 复习问题：一个能帮助读者回顾和理解的问题\n\n请按照以下 JSON 格式返回结果：\n{\n  \"explanation\": \"解释内容\",\n  \"tags\": [\"标签1\", \"标签2\", \"标签3\"],\n  \"question\": \"复习问题\"\n}\n\n摘录内容：\n> {highlight}";

/// 构建 LLM 提示词（支持自定义模板）
pub fn build_enrich_prompt(highlight: &str, context: Option<&str>) -> String {
    build_enrich_prompt_with_template(highlight, context, DEFAULT_ENRICH_PROMPT)
}

/// 使用自定义模板构建 LLM 提示词
pub fn build_enrich_prompt_with_template(highlight: &str, context: Option<&str>, template: &str) -> String {
    // 替换占位符
    let mut prompt = template
        .replace("{highlight}", highlight)
        .to_string();

    // 如果有上下文，添加到末尾
    if let Some(ctx) = context {
        prompt.push_str("\n\n上下文：\n");
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