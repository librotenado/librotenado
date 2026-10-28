// SPDX-License-Identifier: MIT
//! Objects and their types (spec part 0003). Every object of the protocol has a type; it is written where the
//! context does not determine it – on a root object, and in an array that mixes types – and nowhere else.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{CanonicalError, canonicalize, to_canonical_vec};

/// An object of the protocol with its type, e.g. `event` or `account`.
pub trait Object {
    /// The type, written as `"type"` when the object stands on its own.
    const TYPE: &'static str;
}

/// Why JSON is not the expected root object.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectError {
    /// Not JSON that has a canonical encoding.
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
    /// A root object without its type, or of another type.
    #[error("expected an object of type {expected:?}, found {found:?}")]
    Type {
        /// The type expected.
        expected: &'static str,
        /// The type found, if any.
        found: Option<String>,
    },
    /// Of the right type, but not valid content of it.
    #[error("not a valid {0}: {1}")]
    Content(&'static str, String),
}

/// The canonical encoding of `value` as a root object: its members and its `type`.
pub fn to_root<T: Object + Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let mut json = serde_json::to_value(value).map_err(|e| CanonicalError::Json(e.to_string()))?;
    let Value::Object(members) = &mut json else {
        return Err(CanonicalError::Json(format!("a {} is not a JSON object", T::TYPE)));
    };
    members.insert("type".into(), T::TYPE.into());
    to_canonical_vec(&json)
}

/// Reads a root object of type `T` from JSON in any formatting.
pub fn from_root<T: Object + Serialize + DeserializeOwned>(json: &[u8]) -> Result<T, ObjectError> {
    let canonical = canonicalize(json)?;
    let mut value: Value = serde_json::from_slice(&canonical).map_err(|e| CanonicalError::Json(e.to_string()))?;
    let found = value.as_object_mut().and_then(|members| members.remove("type"));
    match found {
        Some(Value::String(found)) if found == T::TYPE => {}
        found => {
            let found = found.map(|t| t.as_str().map_or_else(|| t.to_string(), str::to_owned));
            return Err(ObjectError::Type { expected: T::TYPE, found });
        }
    }
    let object: T = serde_json::from_value(value.clone()).map_err(|e| ObjectError::Content(T::TYPE, e.to_string()))?;
    // Where this version knows the position of a nested object, its type follows from it and is not written. What
    // this version does not know (members of later versions) is not checked: there an array may mix types.
    if let Ok(known) = serde_json::to_value(&object)
        && nested_type(&value, &known, true)
    {
        return Err(ObjectError::Content(T::TYPE, "a nested object carries \"type\"".into()));
    }
    Ok(object)
}

/// Whether `received` has a `type` on a nested object at a position that `known` – this version's reading – has.
fn nested_type(received: &Value, known: &Value, root: bool) -> bool {
    match (received, known) {
        (Value::Object(received), Value::Object(known)) => {
            (!root && received.contains_key("type"))
                || known.iter().any(|(name, k)| received.get(name).is_some_and(|r| nested_type(r, k, false)))
        }
        (Value::Array(received), Value::Array(known)) => {
            received.iter().zip(known).any(|(r, k)| nested_type(r, k, false))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CurrencyDefinition, Money};

    #[test]
    fn a_root_object_carries_its_type() {
        let money = Money::new(1240, "EUR".parse().unwrap()).unwrap();
        let json = to_root(&money).unwrap();
        assert_eq!(json, br#"{"base_units":"1240","currency":"EUR","type":"money"}"#);
        assert_eq!(from_root::<Money>(&json).unwrap(), money);
    }

    #[test]
    fn the_type_must_be_there_and_fit() {
        let untyped = br#"{"base_units":"1240","currency":"EUR"}"#;
        assert!(matches!(from_root::<Money>(untyped), Err(ObjectError::Type { found: None, .. })));
        let other = br#"{"code":"EUR","subdivisions":[100],"type":"money"}"#;
        assert!(matches!(from_root::<CurrencyDefinition>(other), Err(ObjectError::Type { .. })));
        let currency = br#"{"code":"EUR","subdivisions":[100],"type":"currency"}"#;
        assert!(from_root::<CurrencyDefinition>(currency).is_ok());
    }

    #[test]
    fn nested_objects_carry_no_type() {
        let posting = br#"{"type":"posting","owner":"0192f1e4-0000-7000-8000-0000000000a1","account":"0192f1e4-0000-7000-8000-000000000001","amount":{"currency":"EUR","base_units":"5"}}"#;
        assert!(from_root::<crate::Posting>(posting).is_ok());
        let typed = br#"{"type":"posting","owner":"0192f1e4-0000-7000-8000-0000000000a1","account":"0192f1e4-0000-7000-8000-000000000001","amount":{"type":"money","currency":"EUR","base_units":"5"}}"#;
        assert!(matches!(from_root::<crate::Posting>(typed), Err(ObjectError::Content(..))));
        let later = br#"{"type":"posting","owner":"0192f1e4-0000-7000-8000-0000000000a1","account":"0192f1e4-0000-7000-8000-000000000001","amount":{"currency":"EUR","base_units":"5"},"notes":[{"type":"x"}]}"#;
        assert!(from_root::<crate::Posting>(later).is_ok(), "members of later versions are not checked");
    }
}
