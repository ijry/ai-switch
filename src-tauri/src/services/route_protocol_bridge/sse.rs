use serde_json::{json, Value};

pub(super) fn parse_sse_data_records(body: &[u8]) -> Result<Vec<Value>, String> {
    let text = String::from_utf8_lossy(body).replace("\r\n", "\n");
    let mut records = Vec::new();
    for block in text.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        records.push(
            serde_json::from_str::<Value>(&data)
                .map_err(|error| format!("SSE data is invalid JSON: {error}"))?,
        );
    }
    Ok(records)
}

pub(super) fn responses_events_from_completed_response(response: &Value) -> Result<Vec<u8>, String> {
    let response_id = response
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ai_switch");
    let mut output = String::new();
    let mut sequence_number = 0_u64;
    let in_progress = in_progress_response(response);
    push_responses_event(
        &mut output,
        "response.created",
        json!({
            "type": "response.created",
            "sequence_number": sequence_number,
            "response": in_progress
        }),
    )?;
    sequence_number += 1;
    push_responses_event(
        &mut output,
        "response.in_progress",
        json!({
            "type": "response.in_progress",
            "sequence_number": sequence_number,
            "response": in_progress_response(response)
        }),
    )?;
    sequence_number += 1;

    for (output_index, item) in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => emit_message_item(
                &mut output,
                &mut sequence_number,
                output_index,
                item,
                response_id,
            )?,
            Some("function_call") => emit_function_call_item(
                &mut output,
                &mut sequence_number,
                output_index,
                item,
            )?,
            _ => {}
        }
    }

    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let event = match status {
        "failed" => "response.failed",
        "incomplete" => "response.incomplete",
        _ => "response.completed",
    };
    push_responses_event(
        &mut output,
        event,
        json!({
            "type": event,
            "sequence_number": sequence_number,
            "response": response
        }),
    )?;
    Ok(output.into_bytes())
}

fn emit_message_item(
    output: &mut String,
    sequence_number: &mut u64,
    output_index: usize,
    item: &Value,
    response_id: &str,
) -> Result<(), String> {
    let item_id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(response_id);
    let mut started_item = item.clone();
    started_item["status"] = Value::String("in_progress".to_string());
    if let Some(content) = started_item.get_mut("content").and_then(Value::as_array_mut) {
        for part in content {
            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                part["text"] = Value::String(String::new());
            }
        }
    }
    push_responses_event(
        output,
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "sequence_number": *sequence_number,
            "output_index": output_index,
            "item": started_item
        }),
    )?;
    *sequence_number += 1;

    for (content_index, part) in item
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if part.get("type").and_then(Value::as_str) != Some("output_text") {
            continue;
        }
        let text = part.get("text").and_then(Value::as_str).unwrap_or("");
        let mut started_part = part.clone();
        started_part["text"] = Value::String(String::new());
        push_responses_event(
            output,
            "response.content_part.added",
            json!({
                "type": "response.content_part.added",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "content_index": content_index,
                "part": started_part
            }),
        )?;
        *sequence_number += 1;
        if !text.is_empty() {
            push_responses_event(
                output,
                "response.output_text.delta",
                json!({
                    "type": "response.output_text.delta",
                    "sequence_number": *sequence_number,
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": content_index,
                    "delta": text,
                    "logprobs": []
                }),
            )?;
            *sequence_number += 1;
        }
        push_responses_event(
            output,
            "response.output_text.done",
            json!({
                "type": "response.output_text.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "content_index": content_index,
                "text": text,
                "logprobs": []
            }),
        )?;
        *sequence_number += 1;
        push_responses_event(
            output,
            "response.content_part.done",
            json!({
                "type": "response.content_part.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "content_index": content_index,
                "part": part
            }),
        )?;
        *sequence_number += 1;
    }

    push_responses_event(
        output,
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "sequence_number": *sequence_number,
            "output_index": output_index,
            "item": item
        }),
    )?;
    *sequence_number += 1;
    Ok(())
}

fn emit_function_call_item(
    output: &mut String,
    sequence_number: &mut u64,
    output_index: usize,
    item: &Value,
) -> Result<(), String> {
    let item_id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("fc_ai_switch");
    let arguments = item.get("arguments").and_then(Value::as_str).unwrap_or("{}");
    let mut started_item = item.clone();
    started_item["status"] = Value::String("in_progress".to_string());
    started_item["arguments"] = Value::String(String::new());
    push_responses_event(
        output,
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "sequence_number": *sequence_number,
            "output_index": output_index,
            "item": started_item
        }),
    )?;
    *sequence_number += 1;
    if !arguments.is_empty() {
        push_responses_event(
            output,
            "response.function_call_arguments.delta",
            json!({
                "type": "response.function_call_arguments.delta",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "delta": arguments
            }),
        )?;
        *sequence_number += 1;
    }
    push_responses_event(
        output,
        "response.function_call_arguments.done",
        json!({
            "type": "response.function_call_arguments.done",
            "sequence_number": *sequence_number,
            "item_id": item_id,
            "output_index": output_index,
            "arguments": arguments
        }),
    )?;
    *sequence_number += 1;
    push_responses_event(
        output,
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "sequence_number": *sequence_number,
            "output_index": output_index,
            "item": item
        }),
    )?;
    *sequence_number += 1;
    Ok(())
}

fn in_progress_response(response: &Value) -> Value {
    let mut response = response.clone();
    response["status"] = Value::String("in_progress".to_string());
    response["output"] = Value::Array(Vec::new());
    response["output_text"] = Value::String(String::new());
    response
}

fn push_responses_event(output: &mut String, event: &str, value: Value) -> Result<(), String> {
    output.push_str("event: ");
    output.push_str(event);
    output.push('\n');
    output.push_str("data: ");
    output.push_str(
        &serde_json::to_string(&value)
            .map_err(|error| format!("Could not serialize Responses SSE event: {error}"))?,
    );
    output.push_str("\n\n");
    Ok(())
}
