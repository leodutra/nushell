use crate::prelude::*;
use indexmap::IndexMap;
use nu_engine::WholeStreamCommand;
use nu_errors::ShellError;
use nu_protocol::{Primitive, Signature, SyntaxShape, UntaggedValue, Value};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use std::collections::HashSet;
use std::io::Cursor;
use std::io::Write;

pub struct ToXml;

impl WholeStreamCommand for ToXml {
    fn name(&self) -> &str {
        "to xml"
    }

    fn signature(&self) -> Signature {
        Signature::build("to xml").named(
            "pretty",
            SyntaxShape::Int,
            "Formats the XML text with the provided indentation setting",
            Some('p'),
        )
    }

    fn usage(&self) -> &str {
        "Convert table into .xml text"
    }

    fn run(&self, args: CommandArgs) -> Result<OutputStream, ShellError> {
        to_xml(args)
    }
}

pub fn add_attributes<'a>(
    element: &mut quick_xml::events::BytesStart<'a>,
    attributes: &'a IndexMap<String, String>,
) {
    for (k, v) in attributes {
        element.push_attribute((k.as_str(), v.as_str()));
    }
}

pub fn get_attributes(row: &Value) -> Option<IndexMap<String, String>> {
    if let UntaggedValue::Row(r) = &row.value {
        if let Some(v) = r.entries.get("attributes") {
            if let UntaggedValue::Row(a) = &v.value {
                let mut h = IndexMap::new();
                for (k, v) in &a.entries {
                    h.insert(k.clone(), v.convert_to_string());
                }
                return Some(h);
            }
        }
    }
    None
}

pub fn get_children(row: &Value) -> Option<&Vec<Value>> {
    if let UntaggedValue::Row(r) = &row.value {
        if let Some(v) = r.entries.get("children") {
            if let UntaggedValue::Table(t) = &v.value {
                return Some(t);
            }
        }
    }
    None
}

pub fn is_xml_row(row: &Value) -> bool {
    if let UntaggedValue::Row(r) = &row.value {
        let keys: HashSet<&String> = r.keys().collect();
        let children: String = "children".to_string();
        let attributes: String = "attributes".to_string();
        return keys.contains(&children) && keys.contains(&attributes) && keys.len() == 2;
    }
    false
}

pub fn write_xml_events<W: Write>(
    current: &Value,
    writer: &mut quick_xml::Writer<W>,
) -> Result<(), ShellError> {
    match &current.value {
        UntaggedValue::Row(o) => {
            for (k, v) in &o.entries {
                let mut e = BytesStart::owned(k.as_bytes(), k.len());
                if !is_xml_row(v) {
                    return Err(ShellError::labeled_error(
                        "Expected a row with 'children' and 'attributes' columns",
                        "missing 'children' and 'attributes' columns ",
                        &current.tag,
                    ));
                }
                let a = get_attributes(v);
                if let Some(ref a) = a {
                    add_attributes(&mut e, a);
                }
                writer
                    .write_event(Event::Start(e))
                    .expect("Couldn't open XML node");
                let c = get_children(v);
                if let Some(c) = c {
                    for v in c {
                        write_xml_events(v, writer)?;
                    }
                }
                writer
                    .write_event(Event::End(BytesEnd::borrowed(k.as_bytes())))
                    .expect("Couldn't close XML node");
            }
        }
        UntaggedValue::Table(t) => {
            for v in t {
                write_xml_events(v, writer)?;
            }
        }
        _ => {
            let s = current.convert_to_string();
            writer
                .write_event(Event::Text(BytesText::from_plain_str(s.as_str())))
                .expect("Couldn't write XML text");
        }
    }
    Ok(())
}

fn to_xml(args: CommandArgs) -> Result<OutputStream, ShellError> {
    let name_tag = args.call_info.name_tag.clone();
    let name_span = name_tag.span;
    let pretty: Option<Value> = args.get_flag("pretty")?;
    let input: Vec<Value> = args.input.collect();

    let to_process_input = match input.len() {
        x if x > 1 => {
            let tag = input[0].tag.clone();
            vec![Value {
                value: UntaggedValue::Table(input),
                tag,
            }]
        }
        1 => input,
        _ => vec![],
    };

    Ok((to_process_input.into_iter().map(move |value| {
        let mut w = pretty.as_ref().map_or_else(
            || quick_xml::Writer::new(Cursor::new(Vec::new())),
            |p| {
                quick_xml::Writer::new_with_indent(
                    Cursor::new(Vec::new()),
                    b' ',
                    p.value.expect_int() as usize,
                )
            },
        );

        let value_span = value.tag.span;

        match write_xml_events(&value, &mut w) {
            Ok(_) => {
                let b = w.into_inner().into_inner();
                let s = if let Ok(s) = String::from_utf8(b) {
                    s
                } else {
                    return Value::error(ShellError::untagged_runtime_error(
                        "Could not convert a string to utf-8",
                    ));
                };
                UntaggedValue::Primitive(Primitive::String(s)).into_value(&name_tag)
            }
            Err(_) => Value::error(ShellError::labeled_error_with_secondary(
                "Expected a table with XML-compatible structure from pipeline",
                "requires XML-compatible input",
                name_span,
                "originates from here".to_string(),
                value_span,
            )),
        }
    }))
    .into_output_stream())
}

#[cfg(test)]
mod tests {
    use super::ShellError;
    use super::ToXml;

    #[test]
    fn examples_work_as_expected() -> Result<(), ShellError> {
        use crate::examples::test as test_examples;

        test_examples(ToXml {})
    }
}
