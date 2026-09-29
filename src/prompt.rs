//! Apple Books Exporter - LLM Prompt Templates

/// 默认 prompt 模板（用于生成卡片内容）
const DEFAULT_ENRICH_PROMPT: &str = r#"你是一位知识卡片设计师，擅长将书籍摘录转化为简洁有力的知识卡片内容。

## 输入
读者从书中摘录的一段内容。

## 任务
生成适合展示在知识卡片上的简短解读。

## 摘录内容
{highlight}

{note_section}

## 输出要求

### 1. 解读（explanation）
- 用 1-2 句话提炼核心观点或洞见
- 控制在 30-60 字，适合卡片展示
- 避免重复原文，要有提炼和升华
- 可以是：核心观点、行动建议、延展思考

### 2. 标签（tags）
- 3-5 个关键词，用于分类和检索
- 示例方向：主题、领域、方法、场景

### 3. 问题（question）
- 一个启发式问题，帮助回顾和思考
- 控制在 15-25 字

## 输出格式
严格按 JSON 格式输出：

```json
{
  "explanation": "解读内容",
  "tags": ["标签1", "标签2", "标签3"],
  "question": "启发问题"
}
```"#;

const NOTE_SECTION_TEMPLATE: &str = r#"## 读者笔记
{note}"#;

/// 构建 LLM 提示词（支持自定义模板）
pub fn build_enrich_prompt(highlight: &str, context: Option<&str>) -> String {
    build_enrich_prompt_with_template(highlight, context, DEFAULT_ENRICH_PROMPT)
}

/// 使用自定义模板构建 LLM 提示词
pub fn build_enrich_prompt_with_template(highlight: &str, context: Option<&str>, template: &str) -> String {
    // 构建笔记部分（如果有）
    let note_section = match context {
        Some(note) if !note.trim().is_empty() => {
            NOTE_SECTION_TEMPLATE.replace("{note}", note)
        }
        _ => String::new(),
    };

    // 替换占位符
    template
        .replace("{highlight}", highlight)
        .replace("{note_section}", &note_section)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_enrich_prompt() {
        let prompt = build_enrich_prompt("简短模式（short variable declaration）有些限制：", None);
        assert!(prompt.contains("解读"));
        assert!(prompt.contains("标签"));
        assert!(prompt.contains("问题"));
        assert!(prompt.contains("简短模式"));
    }
}