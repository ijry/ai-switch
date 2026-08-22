//! Gemini tool-schema conversion.
//!
//! Gemini's `FunctionDeclaration` exposes two schema channels:
//!
//! - `parameters` — a restricted OpenAPI-flavoured `Schema` proto. The API
//!   frontend rejects unknown fields, so a stray `$schema` or
//!   `additionalProperties` fails the whole request with
//!   `400 Invalid JSON payload ... Cannot find field`.
//! - `parametersJsonSchema` — arbitrary JSON Schema, accepted as-is.
//!
//! Anthropic `input_schema` and OpenAI `parameters` are both JSON Schema, and
//! real-world tools carry keywords the restricted channel cannot express:
//! Claude Code sends `$schema` and `additionalProperties` on every tool, and
//! zod-derived MCP schemas add `$ref`/`$defs`/`oneOf`/`const`. Forwarding those
//! verbatim makes the Gemini bridge unusable for any agentic client.
//!
//! So: normalize the schema, then pick the channel that can carry it.

use serde_json::{json, Map, Value};

/// Which Gemini schema channel a converted schema belongs in.
pub(super) enum GeminiFunctionParameters {
    /// Fits the restricted `Schema` proto.
    Schema(Value),
    /// Needs the richer `parametersJsonSchema` channel.
    JsonSchema(Value),
}

/// Builds a Gemini `functionDeclarations` entry, routing the schema to whichever
/// channel can represent it.
pub(super) fn build_gemini_function_declaration(
    name: &str,
    description: Option<&Value>,
    input_schema: Option<&Value>,
) -> Value {
    let mut declaration = Map::new();
    declaration.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        declaration.insert("description".to_string(), description.clone());
    }

    let schema = input_schema.cloned().unwrap_or_else(|| json!({}));
    match build_gemini_function_parameters(schema) {
        GeminiFunctionParameters::Schema(schema) => {
            declaration.insert("parameters".to_string(), schema);
        }
        GeminiFunctionParameters::JsonSchema(schema) => {
            declaration.insert("parametersJsonSchema".to_string(), schema);
        }
    }
    Value::Object(declaration)
}

pub(super) fn build_gemini_function_parameters(input_schema: Value) -> GeminiFunctionParameters {
    let schema = ensure_object_schema(normalize_json_schema(input_schema));
    if requires_parameters_json_schema(&schema) {
        GeminiFunctionParameters::JsonSchema(schema)
    } else {
        GeminiFunctionParameters::Schema(to_gemini_schema(schema))
    }
}

/// Vertex rejects a declaration whose `parameters` has no explicit
/// `type: "object"`, with `functionDeclaration parameters schema should be of
/// type OBJECT`. No-argument tools (Claude Code's `TodoRead`, for instance)
/// arrive with an empty or type-less schema, so fill the type in.
fn ensure_object_schema(schema: Value) -> Value {
    let Value::Object(mut object) = schema else {
        return schema;
    };
    object
        .entry("type".to_string())
        .or_insert_with(|| json!("object"));
    if object.get("type").and_then(Value::as_str) == Some("object") {
        object
            .entry("properties".to_string())
            .or_insert_with(|| json!({}));
    }
    Value::Object(object)
}

/// Strips keywords that are never valid in either channel. `$schema`/`$id` are
/// metadata about the document rather than the shape, so dropping them loses
/// nothing and often keeps a schema in the cheaper `parameters` channel.
fn normalize_json_schema(schema: Value) -> Value {
    match schema {
        Value::Object(mut object) => {
            object.remove("$schema");
            object.remove("$id");
            object.remove("$comment");

            if let Some(properties) = object
                .get_mut("properties")
                .and_then(|value| value.as_object_mut())
            {
                for value in properties.values_mut() {
                    *value = normalize_json_schema(value.take());
                }
            }
            for key in [
                "items",
                "not",
                "if",
                "then",
                "else",
                "additionalProperties",
                "contains",
                "propertyNames",
            ] {
                if let Some(value) = object.get_mut(key) {
                    *value = normalize_json_schema(value.take());
                }
            }
            for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
                if let Some(values) = object.get_mut(key).and_then(|value| value.as_array_mut()) {
                    for value in values.iter_mut() {
                        *value = normalize_json_schema(value.take());
                    }
                }
            }
            for key in [
                "$defs",
                "definitions",
                "patternProperties",
                "dependentSchemas",
            ] {
                if let Some(entries) = object.get_mut(key).and_then(|value| value.as_object_mut()) {
                    for value in entries.values_mut() {
                        *value = normalize_json_schema(value.take());
                    }
                }
            }
            Value::Object(object)
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(normalize_json_schema).collect())
        }
        other => other,
    }
}

/// True when the schema uses anything the restricted `Schema` proto cannot
/// carry. Unknown keywords count as unsupported: guessing wrong sends an
/// unknown field to the API and 400s, while over-routing to
/// `parametersJsonSchema` is always safe.
fn requires_parameters_json_schema(schema: &Value) -> bool {
    match schema {
        Value::Object(object) => object_requires_parameters_json_schema(object),
        Value::Array(values) => values.iter().any(requires_parameters_json_schema),
        _ => false,
    }
}

fn object_requires_parameters_json_schema(object: &Map<String, Value>) -> bool {
    for (key, value) in object {
        match key.as_str() {
            // A union type (`type: ["string", "null"]`) is not expressible.
            "type" => {
                if value.is_array() {
                    return true;
                }
            }
            // Scalar keywords the restricted channel accepts verbatim.
            "format" | "title" | "description" | "nullable" | "enum" | "maxItems" | "minItems"
            | "required" | "minProperties" | "maxProperties" | "minLength" | "maxLength"
            | "pattern" | "example" | "propertyOrdering" | "default" | "minimum" | "maximum" => {}
            "properties" => {
                let Some(properties) = value.as_object() else {
                    return true;
                };
                if properties.values().any(requires_parameters_json_schema) {
                    return true;
                }
            }
            "items" => {
                if !value.is_object() || requires_parameters_json_schema(value) {
                    return true;
                }
            }
            "anyOf" => {
                let Some(values) = value.as_array() else {
                    return true;
                };
                if values.iter().any(requires_parameters_json_schema) {
                    return true;
                }
            }
            // Everything else — including $ref/$defs/oneOf/allOf/const/
            // additionalProperties/exclusiveMinimum and any keyword added to
            // JSON Schema after this was written.
            _ => return true,
        }
    }
    false
}

/// Projects a schema onto the restricted `Schema` proto, dropping keys it does
/// not define. Only called once `requires_parameters_json_schema` has confirmed
/// nothing load-bearing would be lost.
fn to_gemini_schema(schema: Value) -> Value {
    let Value::Object(object) = schema else {
        return schema;
    };
    let mut result = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "type" | "format" | "title" | "description" | "nullable" | "enum" | "maxItems"
            | "minItems" | "required" | "minProperties" | "maxProperties" | "minLength"
            | "maxLength" | "pattern" | "example" | "propertyOrdering" | "default" | "minimum"
            | "maximum" => {
                result.insert(key, value);
            }
            "properties" => {
                if let Some(properties) = value.as_object() {
                    let converted = properties
                        .iter()
                        .map(|(name, property)| (name.clone(), to_gemini_schema(property.clone())))
                        .collect();
                    result.insert("properties".to_string(), Value::Object(converted));
                }
            }
            "items" if value.is_object() => {
                result.insert("items".to_string(), to_gemini_schema(value));
            }
            "anyOf" => {
                if let Some(values) = value.as_array() {
                    result.insert(
                        "anyOf".to_string(),
                        Value::Array(
                            values
                                .iter()
                                .map(|value| to_gemini_schema(value.clone()))
                                .collect(),
                        ),
                    );
                }
            }
            _ => {}
        }
    }
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_schema_uses_the_restricted_parameters_channel() {
        let declaration = build_gemini_function_declaration(
            "weather",
            Some(&json!("Weather lookup")),
            Some(&json!({
                "type": "object",
                "properties": {"city": {"type": "string", "description": "Target city"}},
                "required": ["city"]
            })),
        );

        assert!(declaration.get("parametersJsonSchema").is_none());
        assert_eq!(
            declaration["parameters"]["properties"]["city"]["type"],
            "string"
        );
        assert_eq!(declaration["parameters"]["required"][0], "city");
    }

    /// Every tool Claude Code sends carries `$schema` and
    /// `additionalProperties`. Before sanitization these reached Gemini's
    /// restricted channel verbatim and 400'd the entire request.
    #[test]
    fn claude_code_style_schema_does_not_leak_json_schema_keywords() {
        let declaration = build_gemini_function_declaration(
            "Read",
            Some(&json!("Read a file")),
            Some(&json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "file_path": {"type": "string"},
                    "offset": {"type": "integer", "exclusiveMinimum": 0}
                },
                "required": ["file_path"]
            })),
        );

        // Rich keywords force the JSON Schema channel rather than being dropped.
        let schema = declaration
            .get("parametersJsonSchema")
            .expect("rich schema must use parametersJsonSchema");
        assert!(declaration.get("parameters").is_none());
        // Document metadata is stripped; shape-bearing keywords survive.
        assert!(schema.get("$schema").is_none(), "$schema must be stripped");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["offset"]["exclusiveMinimum"], 0);
    }

    #[test]
    fn mcp_style_ref_and_oneof_schema_uses_json_schema_channel() {
        let declaration = build_gemini_function_declaration(
            "mcp_tool",
            None,
            Some(&json!({
                "type": "object",
                "properties": {"mode": {"$ref": "#/$defs/Mode"}},
                "$defs": {"Mode": {"oneOf": [{"const": "fast"}, {"const": "slow"}]}}
            })),
        );

        assert!(declaration.get("parametersJsonSchema").is_some());
        assert!(declaration.get("parameters").is_none());
    }

    /// Vertex rejects a type-less `parameters`; no-argument tools must still
    /// produce a valid object schema.
    #[test]
    fn no_argument_tool_gets_an_explicit_object_schema() {
        for schema in [None, Some(json!({})), Some(json!({"properties": {}}))] {
            let declaration = build_gemini_function_declaration("TodoRead", None, schema.as_ref());
            assert_eq!(
                declaration["parameters"]["type"], "object",
                "schema {schema:?} must yield an explicit object type"
            );
            assert!(declaration["parameters"]["properties"].is_object());
        }
    }

    #[test]
    fn union_type_and_nested_violations_are_detected() {
        // A union type is not expressible in the restricted channel.
        let union_type = build_gemini_function_declaration(
            "t",
            None,
            Some(&json!({"type": "object", "properties": {"v": {"type": ["string", "null"]}}})),
        );
        assert!(union_type.get("parametersJsonSchema").is_some());

        // A violation nested inside `items` must escalate the whole schema.
        let nested = build_gemini_function_declaration(
            "t",
            None,
            Some(&json!({
                "type": "object",
                "properties": {
                    "list": {"type": "array", "items": {"type": "object", "additionalProperties": false}}
                }
            })),
        );
        assert!(nested.get("parametersJsonSchema").is_some());
    }

    #[test]
    fn unknown_keywords_are_treated_as_unsupported() {
        // A keyword this code has never heard of must route to the safe channel
        // rather than being silently forwarded to the restricted one.
        let declaration = build_gemini_function_declaration(
            "t",
            None,
            Some(&json!({"type": "object", "properties": {}, "futureKeyword": true})),
        );
        assert!(declaration.get("parametersJsonSchema").is_some());
    }
}
