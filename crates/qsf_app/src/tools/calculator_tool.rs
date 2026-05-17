use anyhow::{Result, bail};

use crate::models::ModelToolDefinition;

use super::tool_registry::{Tool, ToolMetadata};
use super::tool_request::{ToolCategory, ToolRequest, ToolSideEffectLevel};
use super::tool_result::ToolResult;

pub const CALCULATOR_TOOL_NAME: &str = "calculator";

#[derive(Default)]
pub struct CalculatorTool;

impl Tool for CalculatorTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: CALCULATOR_TOOL_NAME,
            description: "Evaluate deterministic arithmetic expressions without side effects",
            category: ToolCategory::ComputeOnly,
            side_effect_level: ToolSideEffectLevel::None,
        }
    }

    fn execute(&self, request: &ToolRequest) -> Result<ToolResult> {
        let value = evaluate_expression(&request.input)?;
        let output_text = format_number(value);

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ComputeOnly,
            side_effect_level: ToolSideEffectLevel::None,
            input: request.input.clone(),
            output_text: output_text.clone(),
            numeric_value: Some(value),
            observation_summary: format!(
                "Calculator observed that {} = {}.",
                request.input, output_text
            ),
        })
    }

    fn model_tool_definition(&self) -> Option<ModelToolDefinition> {
        Some(ModelToolDefinition::new(
            CALCULATOR_TOOL_NAME,
            "Evaluate a deterministic arithmetic expression and return the numeric result.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Arithmetic expression with +, -, *, /, parentheses, and decimal numbers."
                    }
                },
                "required": ["expression"],
                "additionalProperties": false
            }),
        ))
    }
}

fn evaluate_expression(expression: &str) -> Result<f64> {
    let mut parser = ExpressionParser::new(expression);
    let value = parser.parse_expression()?;
    parser.skip_whitespace();

    if !parser.is_at_end() {
        bail!(
            "unexpected trailing input at position {} in `{}`",
            parser.position(),
            expression
        );
    }

    Ok(value)
}

fn format_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        return format!("{value:.0}");
    }

    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

struct ExpressionParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> ExpressionParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            position: 0,
        }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn consume(&mut self, expected: u8) -> bool {
        self.skip_whitespace();
        if self.peek() == Some(expected) {
            self.position += 1;
            return true;
        }
        false
    }

    fn parse_expression(&mut self) -> Result<f64> {
        let mut value = self.parse_term()?;

        loop {
            if self.consume(b'+') {
                value += self.parse_term()?;
            } else if self.consume(b'-') {
                value -= self.parse_term()?;
            } else {
                break;
            }
        }

        Ok(value)
    }

    fn parse_term(&mut self) -> Result<f64> {
        let mut value = self.parse_factor()?;

        loop {
            if self.consume(b'*') {
                value *= self.parse_factor()?;
            } else if self.consume(b'/') {
                let divisor = self.parse_factor()?;
                if divisor.abs() < f64::EPSILON {
                    bail!("division by zero is not allowed");
                }
                value /= divisor;
            } else {
                break;
            }
        }

        Ok(value)
    }

    fn parse_factor(&mut self) -> Result<f64> {
        self.skip_whitespace();

        if self.consume(b'(') {
            let value = self.parse_expression()?;
            if !self.consume(b')') {
                bail!("expected closing ')' at position {}", self.position);
            }
            return Ok(value);
        }

        if self.consume(b'-') {
            return Ok(-self.parse_factor()?);
        }

        if self.consume(b'+') {
            return self.parse_factor();
        }

        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<f64> {
        self.skip_whitespace();
        let start = self.position;
        let mut seen_digit = false;
        let mut seen_decimal = false;

        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                seen_digit = true;
                self.position += 1;
                continue;
            }

            if byte == b'.' && !seen_decimal {
                seen_decimal = true;
                self.position += 1;
                continue;
            }

            break;
        }

        if !seen_digit {
            bail!("expected number at position {}", start);
        }

        let literal = std::str::from_utf8(&self.input[start..self.position])?;
        literal.parse::<f64>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::CalculatorTool;
    use crate::tools::{Tool, ToolRequest};

    #[test]
    fn calculator_respects_operator_precedence() {
        let tool = CalculatorTool;
        let request = ToolRequest::calculator("2 + 3 * 4", "test");

        let result = tool.execute(&request).unwrap();

        assert_eq!(result.output_text, "14");
        assert_eq!(result.numeric_value, Some(14.0));
    }

    #[test]
    fn calculator_rejects_invalid_input() {
        let tool = CalculatorTool;
        let request = ToolRequest::calculator("2 + (3 *", "test");

        let error = tool.execute(&request).unwrap_err();

        assert!(
            error.to_string().contains("expected number")
                || error.to_string().contains("expected closing ')'")
        );
    }
}
