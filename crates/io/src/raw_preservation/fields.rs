//! Semantic field paths retain subclasses, application groups, xdata applications and repeated values.

use std::collections::BTreeMap;

use cadcraft_doc::{Handle, RawTag};
use cadcraft_dxf::{Kind, kind_of};

const MAX_SEMANTIC_BYTES: usize = 64 << 20;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Field {
    pub class: String,
    pub groups: Vec<String>,
    pub application: Option<String>,
    pub embedded: usize,
    pub code: i32,
    pub index: usize,
}

impl Field {
    pub fn label(&self) -> String {
        format!(
            "{}/{}/{}/{} group {}[{}]",
            self.class,
            self.groups.join("/"),
            self.application.as_deref().unwrap_or(""),
            self.embedded,
            self.code,
            self.index
        )
    }
    pub fn common(&self) -> bool {
        self.class == "AcDbEntity" && self.groups.is_empty() && self.application.is_none() && self.embedded == 0
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Text(String),
    Real(f64),
    Integer(i64),
    Handle(Handle),
    Binary(Vec<u8>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Handle(a), Self::Handle(b)) => a == b,
            (Self::Binary(a), Self::Binary(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FieldError {
    pub field: String,
    pub reason: String,
}

pub(crate) type Fields = BTreeMap<Field, Value>;

pub(crate) use cadcraft_doc::foreign::is_pointer as reference;

fn scalar(code: i32, text: &str, source_text: bool) -> Result<Value, String> {
    if code == 5 || reference(code) || (320..=329).contains(&code) {
        return Handle::parse_hex(text).map(Value::Handle).ok_or_else(|| "the required handle cannot be decoded".into());
    }
    match kind_of(code) {
        Kind::Real => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(Value::Real)
            .ok_or_else(|| "the required finite number cannot be decoded".into()),
        Kind::Int16 | Kind::Int32 | Kind::Int64 | Kind::Bool => {
            text.trim().parse::<i64>().map(Value::Integer).map_err(|_| "the required integer cannot be decoded".into())
        }
        Kind::Hex => {
            let bytes = text.trim().as_bytes();
            if !bytes.len().is_multiple_of(2) {
                return Err("the binary field has an incomplete byte".into());
            }
            let mut decoded = Vec::with_capacity(bytes.len() / 2);
            for pair in bytes.as_chunks::<2>().0 {
                let digit = |value: u8| char::from(value).to_digit(16).and_then(|value| u8::try_from(value).ok());
                let (Some(high), Some(low)) = (digit(pair[0]), digit(pair[1])) else { return Err("the binary field is not hexadecimal".into()) };
                decoded.push(high * 16 + low);
            }
            Ok(Value::Binary(decoded))
        }
        Kind::Str => Ok(Value::Text(if source_text { cadcraft_dxf::unescape_unicode(text) } else { text.into() })),
    }
}

fn insert(fields: &mut Fields, counts: &mut BTreeMap<Field, usize>, mut field: Field, value: Value) {
    let mut key = field.clone();
    // Foreign payloads are ordered typed sequences, not independent fields with per-code indices.
    if !field.groups.is_empty() || field.application.is_some() || field.embedded != 0 || field.class == "AcDbXrecord" {
        key.code = 0;
    }
    let index = counts.entry(key).or_default();
    field.index = *index;
    *index += 1;
    fields.insert(field, value);
}

pub(crate) fn parse(tags: &[RawTag], source_text: bool) -> Result<Fields, FieldError> {
    let mut fields = Fields::new();
    let mut counts = BTreeMap::new();
    let mut groups: Vec<String> = Vec::new();
    let mut group_counts = BTreeMap::<(Vec<String>, String), usize>::new();
    let mut class = String::new();
    let mut application = None;
    let mut embedded = 0usize;
    let mut owner_seen = false;
    let mut semantic_bytes = 0usize;
    for tag in tags {
        let path_bytes = groups
            .iter()
            .fold(class.len(), |size, group| size.saturating_add(group.len()))
            .saturating_add(application.as_ref().map_or(0, String::len));
        semantic_bytes = semantic_bytes.saturating_add(path_bytes).saturating_add(tag.value.len());
        if semantic_bytes > MAX_SEMANTIC_BYTES {
            return Err(FieldError { field: tag.code.to_string(), reason: "the semantic field path and value budget is exceeded".into() });
        }
        if tag.code == 102 && tag.value.trim() == "}" {
            if groups.pop().is_none() {
                return Err(FieldError { field: "102".into(), reason: "the decoded application group is unbalanced".into() });
            }
            continue;
        }
        if tag.code == 102 && tag.value.trim().starts_with('{') {
            if groups.len() >= cadcraft_doc::foreign::MAX_FOREIGN_DEPTH {
                return Err(FieldError { field: "102".into(), reason: "the semantic application-group depth is exceeded".into() });
            }
            let name = tag.value.trim().trim_start_matches('{').to_string();
            let index = group_counts.entry((groups.clone(), name.clone())).or_default();
            groups.push(format!("{name}[{index}]"));
            *index += 1;
        }
        let owner = tag.code == 330 && groups.is_empty() && application.is_none() && !owner_seen;
        owner_seen |= owner;
        let header = tag.code == 5 || tag.code == 100 || owner || !groups.is_empty();
        let field = Field {
            class: if header { String::new() } else { class.clone() },
            groups: groups.clone(),
            application: application.clone(),
            embedded,
            code: tag.code,
            index: 0,
        };
        let text = if tag.code == 102 { tag.value.trim() } else { &tag.value };
        let value = scalar(tag.code, text, source_text).map_err(|reason| FieldError { field: field.label(), reason })?;
        insert(&mut fields, &mut counts, field, value);
        if tag.code == 100 && groups.is_empty() && application.is_none() {
            class = tag.value.clone();
        }
        if tag.code == 1001 {
            application = Some(tag.value.clone());
        }
        if tag.code == 101 {
            embedded += 1;
        }
    }
    if !groups.is_empty() {
        return Err(FieldError { field: "102".into(), reason: "the decoded application group is not closed".into() });
    }
    Ok(fields)
}

/// Accepted wire equivalences, with their reasons. No rule ignores an unknown field.
/// - Numeric spelling: DXF text and the finite binary64/integer value it encodes are equal.
/// - Handle spelling: leading zeros and hex case do not change the database identity.
/// - Unicode escapes: DXF escapes and their decoded characters encode the same string.
/// - Byte spelling: hexadecimal case does not change binary field bytes.
/// - Common-property overrides and reference remapping are checked separately against the model.
///
/// Optional-field defaults are listed here only after a documented format rule justifies them.
pub(crate) fn default_value(kind: &str, field: &Field, source: &Fields) -> Option<Value> {
    if !field.groups.is_empty() || field.application.is_some() || field.embedded != 0 {
        return None;
    }
    // DXF Reference, ATTRIB: AcDbAttribute's first 280 is version 0 (R2010),
    // and its second 280 is the lock-position flag. ODA20.4.4 has no version
    // byte before R2010 or lock bit before R2007, so both absent values are zero.
    // Binary DXF inspection promotes older output to R2010 and emits those zeros.
    // Nonzero values still compare strictly; a third 280 is not a documented field.
    // https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-7DD8B495-C3F8-48CD-A766-14F9D7D0DD9B.htm
    if kind == "ATTRIB" && field.class == "AcDbAttribute" && field.code == 280 && field.index < 2 {
        return Some(Value::Integer(0));
    }
    if field.index != 0 {
        return None;
    }
    let zero_alignment = |class: &str, code: i32| {
        source
            .iter()
            .find(|(field, _)| field.class == class && field.code == code && field.groups.is_empty() && field.application.is_none())
            .is_none_or(|(_, value)| *value == Value::Integer(0))
    };
    match (kind, field.class.as_str(), field.code) {
        // DXF Reference, INSERT: unit scale, zero rotation/spacing and one row/column are implicit.
        ("INSERT", "AcDbBlockReference", 41..=43) => Some(Value::Real(1.0)),
        ("INSERT", "AcDbBlockReference", 44 | 45 | 50) => Some(Value::Real(0.0)),
        ("INSERT", "AcDbBlockReference", 70 | 71) => Some(Value::Integer(1)),
        // TEXT/ATTRIB: the second alignment point is unused for left/baseline text. Only zero is equivalent here.
        ("ATTRIB", "AcDbText", 11 | 21 | 31) if zero_alignment("AcDbText", 72) && zero_alignment("AcDbAttribute", 74) => Some(Value::Real(0.0)),
        // DXF Reference, TEXT: zero oblique/generation flags and the default +Z extrusion.
        ("ATTRIB", "AcDbText", 51 | 210 | 220) => Some(Value::Real(0.0)),
        ("ATTRIB", "AcDbText", 230) => Some(Value::Real(1.0)),
        ("ATTRIB", "AcDbText", 71) => Some(Value::Integer(0)),
        // DXF Reference, VERTEX: the optional curve-fit tangent defaults to zero.
        ("VERTEX", "AcDb2dVertex", 50) => Some(Value::Real(0.0)),
        // DXF Reference, DICTIONARY: absent hard-owner flag means soft ownership.
        ("DICTIONARY", "AcDbDictionary", 280) => Some(Value::Integer(0)),
        _ => None,
    }
}
