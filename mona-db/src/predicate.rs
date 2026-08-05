use std::cmp::Ordering;

use bson::{Bson, Document};

use crate::error::{Error, Result};

/// Compiled MongoDB-style query predicate (top-level fields + supported `$` ops).
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Always,
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
    Not(Box<Predicate>),
    Field { name: String, op: FieldOp },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldOp {
    Eq(Bson),
    Ne(Bson),
    Gt(Bson),
    Gte(Bson),
    Lt(Bson),
    Lte(Bson),
    In(Vec<Bson>),
    Nin(Vec<Bson>),
    Exists(bool),
}

impl Predicate {
    pub(crate) fn parse(query: &Document) -> Result<Self> {
        parse_query(query)
    }

    pub(crate) fn matches(&self, doc: &Document) -> bool {
        match self {
            Self::Always => true,
            Self::And(parts) => parts.iter().all(|p| p.matches(doc)),
            Self::Or(parts) => parts.iter().any(|p| p.matches(doc)),
            Self::Not(inner) => !inner.matches(doc),
            Self::Field { name, op } => field_matches(doc.get(name), op),
        }
    }

    /// If this predicate is solely `_id` equality, return that id.
    pub(crate) fn as_id_eq(&self) -> Option<&Bson> {
        match self {
            Self::Field { name, op: FieldOp::Eq(id) } if name == "_id" => Some(id),
            _ => None,
        }
    }

    /// If an `_id` equality conjunct is present, return it (for point-get fast path).
    pub(crate) fn extract_id_eq(&self) -> Option<&Bson> {
        match self {
            Self::Field { name, op: FieldOp::Eq(id) } if name == "_id" => Some(id),
            Self::And(parts) => {
                let mut found = None;
                for part in parts {
                    if let Some(id) = part.as_id_eq() {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(id);
                    }
                }
                found
            }
            _ => None,
        }
    }
}

fn parse_query(query: &Document) -> Result<Predicate> {
    if query.is_empty() {
        return Ok(Predicate::Always);
    }

    let mut parts = Vec::new();

    for (key, value) in query.iter() {
        if key.starts_with('$') {
            match key.as_str() {
                "$and" => parts.push(parse_logical_array(value, "$and", true)?),
                "$or" => parts.push(parse_logical_array(value, "$or", false)?),
                "$not" => {
                    let Bson::Document(inner) = value else {
                        return Err(Error::CommandParse(
                            "field '$not' must be a document".into(),
                        ));
                    };
                    parts.push(Predicate::Not(Box::new(parse_query(inner)?)));
                }
                other => {
                    return Err(Error::CommandParse(format!(
                        "unsupported query operator: '{other}'"
                    )));
                }
            }
        } else {
            parts.push(parse_field(key, value)?);
        }
    }

    Ok(collapse_and(parts))
}

fn parse_logical_array(value: &Bson, name: &str, is_and: bool) -> Result<Predicate> {
    let Bson::Array(items) = value else {
        return Err(Error::CommandParse(format!(
            "field '{name}' must be an array"
        )));
    };
    if items.is_empty() {
        return Err(Error::CommandParse(format!(
            "field '{name}' must be a non-empty array"
        )));
    }

    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        let Bson::Document(doc) = item else {
            return Err(Error::CommandParse(format!(
                "field '{name}' must contain documents"
            )));
        };
        parts.push(parse_query(doc)?);
    }

    Ok(if is_and {
        collapse_and(parts)
    } else {
        Predicate::Or(parts)
    })
}

fn parse_field(name: &str, value: &Bson) -> Result<Predicate> {
    match value {
        Bson::Document(ops) if is_operator_doc(ops) => {
            let mut parts = Vec::new();
            for (op_key, op_value) in ops.iter() {
                if !op_key.starts_with('$') {
                    return Err(Error::CommandParse(format!(
                        "mixed operator/equality document for field '{name}' is unsupported"
                    )));
                }
                parts.push(parse_field_op(name, op_key, op_value)?);
            }
            Ok(collapse_and(parts))
        }
        other => Ok(Predicate::Field {
            name: name.to_string(),
            op: FieldOp::Eq(other.clone()),
        }),
    }
}

fn is_operator_doc(doc: &Document) -> bool {
    !doc.is_empty() && doc.keys().all(|k| k.starts_with('$'))
}

fn parse_field_op(field: &str, op: &str, value: &Bson) -> Result<Predicate> {
    let field_op = match op {
        "$eq" => FieldOp::Eq(value.clone()),
        "$ne" => FieldOp::Ne(value.clone()),
        "$gt" => FieldOp::Gt(value.clone()),
        "$gte" => FieldOp::Gte(value.clone()),
        "$lt" => FieldOp::Lt(value.clone()),
        "$lte" => FieldOp::Lte(value.clone()),
        "$in" => FieldOp::In(parse_bson_array(value, "$in")?),
        "$nin" => FieldOp::Nin(parse_bson_array(value, "$nin")?),
        "$exists" => FieldOp::Exists(parse_bool(value, "$exists")?),
        "$not" => {
            let Bson::Document(inner) = value else {
                return Err(Error::CommandParse(
                    "field operator '$not' must be a document".into(),
                ));
            };
            if !is_operator_doc(inner) {
                return Err(Error::CommandParse(
                    "field operator '$not' must contain operators".into(),
                ));
            }
            let mut parts = Vec::new();
            for (inner_op, inner_val) in inner.iter() {
                parts.push(parse_field_op(field, inner_op, inner_val)?);
            }
            return Ok(Predicate::Not(Box::new(collapse_and(parts))));
        }
        other => {
            return Err(Error::CommandParse(format!(
                "unsupported field operator: '{other}'"
            )));
        }
    };

    Ok(Predicate::Field {
        name: field.to_string(),
        op: field_op,
    })
}

fn parse_bson_array(value: &Bson, name: &str) -> Result<Vec<Bson>> {
    match value {
        Bson::Array(items) => Ok(items.clone()),
        _ => Err(Error::CommandParse(format!(
            "field '{name}' must be an array"
        ))),
    }
}

fn parse_bool(value: &Bson, name: &str) -> Result<bool> {
    match value {
        Bson::Boolean(v) => Ok(*v),
        _ => Err(Error::CommandParse(format!(
            "field '{name}' must be a boolean"
        ))),
    }
}

fn collapse_and(mut parts: Vec<Predicate>) -> Predicate {
    match parts.len() {
        0 => Predicate::Always,
        1 => parts.pop().unwrap(),
        _ => Predicate::And(parts),
    }
}

fn field_matches(actual: Option<&Bson>, op: &FieldOp) -> bool {
    match op {
        FieldOp::Exists(want) => actual.is_some() == *want,
        FieldOp::Eq(expected) => actual == Some(expected),
        FieldOp::Ne(expected) => actual != Some(expected),
        FieldOp::In(values) => actual.is_some_and(|v| values.iter().any(|x| x == v)),
        FieldOp::Nin(values) => actual.is_none_or(|v| values.iter().all(|x| x != v)),
        FieldOp::Gt(bound) => compare(actual, bound) == Some(Ordering::Greater),
        FieldOp::Gte(bound) => matches!(
            compare(actual, bound),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        FieldOp::Lt(bound) => compare(actual, bound) == Some(Ordering::Less),
        FieldOp::Lte(bound) => matches!(
            compare(actual, bound),
            Some(Ordering::Less | Ordering::Equal)
        ),
    }
}

fn compare(actual: Option<&Bson>, bound: &Bson) -> Option<Ordering> {
    let actual = actual?;
    cmp_bson(actual, bound)
}

/// Compare BSON values for query operators. Numbers coerce across Int32/Int64/Double;
/// other types compare only when both sides share a comparable shape.
fn cmp_bson(a: &Bson, b: &Bson) -> Option<Ordering> {
    if let (Some(x), Some(y)) = (as_f64(a), as_f64(b)) {
        return x.partial_cmp(&y);
    }

    match (a, b) {
        (Bson::String(x), Bson::String(y)) => Some(x.cmp(y)),
        (Bson::Boolean(x), Bson::Boolean(y)) => Some(x.cmp(y)),
        (Bson::ObjectId(x), Bson::ObjectId(y)) => Some(x.bytes().cmp(&y.bytes())),
        (Bson::Null, Bson::Null) | (Bson::Undefined, Bson::Undefined) => Some(Ordering::Equal),
        _ if a == b => Some(Ordering::Equal),
        _ => None,
    }
}

fn as_f64(value: &Bson) -> Option<f64> {
    match value {
        Bson::Int32(n) => Some(*n as f64),
        Bson::Int64(n) => Some(*n as f64),
        Bson::Double(n) => Some(*n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn equality_and_comparison() {
        let pred = Predicate::parse(&doc! { "score": { "$gte": 10, "$lt": 20 } }).unwrap();
        assert!(pred.matches(&doc! { "score": 10 }));
        assert!(pred.matches(&doc! { "score": 19 }));
        assert!(!pred.matches(&doc! { "score": 20 }));
        assert!(!pred.matches(&doc! { "score": 9 }));
    }

    #[test]
    fn in_and_exists() {
        let pred = Predicate::parse(&doc! {
            "name": { "$in": ["alice", "bob"] },
            "tag": { "$exists": true }
        })
        .unwrap();
        assert!(pred.matches(&doc! { "name": "alice", "tag": 1 }));
        assert!(!pred.matches(&doc! { "name": "carol", "tag": 1 }));
        assert!(!pred.matches(&doc! { "name": "alice" }));
    }

    #[test]
    fn and_or_not() {
        let pred = Predicate::parse(&doc! {
            "$or": [
                { "name": "alice" },
                { "score": { "$gt": 50 } }
            ]
        })
        .unwrap();
        assert!(pred.matches(&doc! { "name": "alice", "score": 1 }));
        assert!(pred.matches(&doc! { "name": "bob", "score": 51 }));
        assert!(!pred.matches(&doc! { "name": "bob", "score": 50 }));

        let not_pred = Predicate::parse(&doc! {
            "score": { "$not": { "$lt": 10 } }
        })
        .unwrap();
        assert!(not_pred.matches(&doc! { "score": 10 }));
        assert!(!not_pred.matches(&doc! { "score": 9 }));
    }

    #[test]
    fn rejects_unknown_operator() {
        let err = Predicate::parse(&doc! { "score": { "$mod": [2, 0] } }).unwrap_err();
        assert!(err.to_string().contains("unsupported field operator"));
    }
}
