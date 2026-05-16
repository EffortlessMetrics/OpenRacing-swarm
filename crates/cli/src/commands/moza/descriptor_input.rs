//! Operator-supplied HID report descriptor input parsing.
//!
//! This module owns the CLI-facing parsing logic for inline/file descriptor
//! sources so the main Moza command module can focus on command orchestration.

use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

use super::{
    bytes_hex_compact, parse_hex_bytes, parse_hex_u8_token,
    reject_unsupported_report_descriptor_bytes,
};

pub(super) fn read_report_descriptor_hex_file(path: &Path) -> Result<String> {
    let raw = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    let text = match String::from_utf8(raw) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    };
    let bytes = extract_hex_bytes_from_descriptor_text(&text)?;
    if bytes.is_empty() {
        return Err(anyhow!(
            "no HID report descriptor bytes found in '{}'; export or paste the actual Report Descriptor byte block, for example lines like '0000: 05 01 09 04 ...' or a compact hex descriptor. A USBTreeView device/interface summary, wDescriptorLength value, ERROR_INVALID_PARAMETER descriptor-read failure, or Windows HidP KDR collection/preparsed descriptor is not enough.",
            path.display()
        ));
    }
    Ok(bytes_hex_compact(&bytes))
}

pub(super) fn read_report_descriptor_bin_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    if bytes.is_empty() {
        return Err(anyhow!(
            "no HID report descriptor bytes found in '{}'; provide the raw binary HID report_descriptor file, for example Linux /sys/class/hidraw/<node>/device/report_descriptor.",
            path.display()
        ));
    }
    reject_unsupported_report_descriptor_bytes(&bytes)?;
    Ok(bytes_hex_compact(&bytes))
}

fn extract_hex_bytes_from_descriptor_text(text: &str) -> Result<Vec<u8>> {
    if let Some(bytes) = extract_explicit_report_descriptor_block(text)? {
        return Ok(bytes);
    }
    if looks_like_usbtreeview_summary(text) {
        return Ok(Vec::new());
    }

    let mut bytes = Vec::new();
    for line in text.lines() {
        if let Some(mut line_bytes) = extract_hex_bytes_from_descriptor_line(line)? {
            bytes.append(&mut line_bytes);
        }
    }
    Ok(bytes)
}

fn extract_explicit_report_descriptor_block(text: &str) -> Result<Option<Vec<u8>>> {
    let mut in_report_descriptor = false;
    let mut bytes = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if is_report_descriptor_heading(trimmed) {
            in_report_descriptor = true;
            continue;
        }
        if !in_report_descriptor {
            continue;
        }
        if !bytes.is_empty() && starts_next_usbtreeview_descriptor_block(trimmed) {
            break;
        }
        if let Some(mut line_bytes) =
            extract_hex_bytes_from_descriptor_line_with_context(line, true)?
        {
            bytes.append(&mut line_bytes);
        }
    }

    if in_report_descriptor {
        Ok(Some(bytes))
    } else {
        Ok(None)
    }
}

fn is_report_descriptor_heading(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("report descriptor")
}

fn starts_next_usbtreeview_descriptor_block(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("interface descriptor")
        || lower.contains("endpoint descriptor")
        || lower.contains("hid descriptor")
        || lower.contains("string descriptor")
        || lower.contains("device descriptor")
        || lower.contains("configuration descriptor")
}

fn looks_like_usbtreeview_summary(text: &str) -> bool {
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("data (hexdump)")
            || lower.contains("usb device")
            || lower.contains("interface descriptor")
            || lower.contains("hid descriptor")
            || lower.contains("bdescriptortype")
            || lower.contains("error reading descriptor")
    })
}

fn extract_hex_bytes_from_descriptor_line(line: &str) -> Result<Option<Vec<u8>>> {
    extract_hex_bytes_from_descriptor_line_with_context(line, false)
}

fn extract_hex_bytes_from_descriptor_line_with_context(
    line: &str,
    allow_hexdump_prefix: bool,
) -> Result<Option<Vec<u8>>> {
    let without_comments = line.split("//").next().unwrap_or_default().trim();
    if without_comments.is_empty() {
        return Ok(None);
    }

    let Some(candidate) = descriptor_byte_candidate(without_comments, allow_hexdump_prefix) else {
        return Ok(None);
    };
    let tokens = candidate
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(None);
    }

    if tokens.len() == 1 && is_compact_hex_byte_string(tokens[0]) {
        return parse_hex_bytes(tokens[0])
            .map(Some)
            .map_err(|e| anyhow!("invalid descriptor byte line '{without_comments}': {e}"));
    }

    if !tokens.iter().all(|token| is_hex_byte_token(token)) {
        if allow_hexdump_prefix {
            let prefix_tokens = tokens
                .iter()
                .copied()
                .take_while(|token| is_hex_byte_token(token))
                .collect::<Vec<_>>();
            if !prefix_tokens.is_empty() {
                return prefix_tokens
                    .iter()
                    .map(|token| {
                        parse_hex_u8_token(token).map_err(|e| {
                            anyhow!("invalid descriptor byte line '{without_comments}': {e}")
                        })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(Some);
            }
        }
        return Ok(None);
    }

    tokens
        .iter()
        .map(|token| {
            parse_hex_u8_token(token)
                .map_err(|e| anyhow!("invalid descriptor byte line '{without_comments}': {e}"))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn descriptor_byte_candidate(line: &str, allow_hexdump_prefix: bool) -> Option<&str> {
    if let Some((prefix, suffix)) = line.split_once(':') {
        let prefix = prefix.trim();
        let suffix = suffix.trim();
        if is_hex_offset_token(prefix)
            || prefix.to_ascii_lowercase().contains("report descriptor")
            || (allow_hexdump_prefix && prefix.eq_ignore_ascii_case("data (hexdump)"))
        {
            return Some(suffix);
        }
        return None;
    }

    if allow_hexdump_prefix {
        let mut parts = line.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or_default().trim();
        let rest = parts.next().unwrap_or_default().trim();
        if is_hexdump_offset_column_token(first) && !rest.is_empty() {
            return Some(rest);
        }
    }

    Some(line)
}

fn is_hex_offset_token(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    !value.is_empty()
        && token
            .trim()
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        && value.len() <= 8
        && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_hexdump_offset_column_token(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    value.len() >= 4
        && value.len() <= 8
        && token
            .trim()
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_hex_byte_token(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    value.len() == 2 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_compact_hex_byte_string(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    value.len() > 2 && value.len().is_multiple_of(2) && value.chars().all(|c| c.is_ascii_hexdigit())
}
