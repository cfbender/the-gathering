//! The subset of Erlang's external term format (`:erlang.term_to_binary/1`) that stored
//! import identities are hashed from: CSV game `external_id`s and Google Sheet row keys
//! (`sheet:<key>` and `sheet_import_receipts.key`), first written by the Elixir releases (up
//! to 0.2).
//!
//! This is a frozen hash encoding, not a general serializer: it must not change. Encoding the
//! same terms byte for byte is what lets re-imports recognize games imported earlier; any
//! difference silently duplicates them.

/// A term to encode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term<'a> {
    /// An atom (`nil`, `:blank_kills_zero`).
    Atom(&'a str),
    /// An integer.
    Int(i64),
    /// A UTF-8 binary (an Elixir string).
    Binary(&'a str),
    /// A tuple.
    Tuple(Vec<Term<'a>>),
    /// A proper list.
    List(Vec<Term<'a>>),
    /// A map, encoded in the given key order (the BEAM's own order for the map).
    Map(Vec<(Term<'a>, Term<'a>)>),
}

impl<'a> Term<'a> {
    /// `nil`.
    pub const NIL: Self = Self::Atom("nil");

    /// A binary, or `nil` for `None`.
    pub fn binary_or_nil(value: Option<&'a str>) -> Self {
        value.map_or(Self::NIL, Self::Binary)
    }

    /// An integer, or `nil` for `None`.
    pub fn int_or_nil(value: Option<i64>) -> Self {
        value.map_or(Self::NIL, Self::Int)
    }
}

fn length(out: &mut Vec<u8>, len: usize) {
    out.extend_from_slice(&u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes());
}

fn write(out: &mut Vec<u8>, term: &Term<'_>) {
    match term {
        Term::Atom(name) => {
            let bytes = name.as_bytes();
            match u8::try_from(bytes.len()) {
                Ok(len) => {
                    out.push(119); // SMALL_ATOM_UTF8_EXT
                    out.push(len);
                }
                Err(_) => {
                    out.push(118); // ATOM_UTF8_EXT
                    out.extend_from_slice(
                        &u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_be_bytes(),
                    );
                }
            }
            out.extend_from_slice(bytes);
        }
        Term::Int(value) => {
            if let Ok(small) = u8::try_from(*value) {
                out.push(97); // SMALL_INTEGER_EXT
                out.push(small);
            } else if let Ok(int) = i32::try_from(*value) {
                out.push(98); // INTEGER_EXT
                out.extend_from_slice(&int.to_be_bytes());
            } else {
                out.push(110); // SMALL_BIG_EXT
                let magnitude = value.unsigned_abs().to_le_bytes();
                let digits: Vec<u8> = {
                    let mut digits = magnitude.to_vec();
                    while digits.last() == Some(&0) {
                        digits.pop();
                    }
                    digits
                };
                out.push(u8::try_from(digits.len()).unwrap_or(8));
                out.push(u8::from(*value < 0));
                out.extend_from_slice(&digits);
            }
        }
        Term::Binary(text) => {
            out.push(109); // BINARY_EXT
            length(out, text.len());
            out.extend_from_slice(text.as_bytes());
        }
        Term::Tuple(items) => {
            match u8::try_from(items.len()) {
                Ok(arity) => {
                    out.push(104); // SMALL_TUPLE_EXT
                    out.push(arity);
                }
                Err(_) => {
                    out.push(105); // LARGE_TUPLE_EXT
                    length(out, items.len());
                }
            }
            for item in items {
                write(out, item);
            }
        }
        Term::List(items) => {
            if !items.is_empty() {
                out.push(108); // LIST_EXT
                length(out, items.len());
                for item in items {
                    write(out, item);
                }
            }
            out.push(106); // NIL_EXT
        }
        Term::Map(pairs) => {
            out.push(116); // MAP_EXT
            length(out, pairs.len());
            for (key, value) in pairs {
                write(out, key);
                write(out, value);
            }
        }
    }
}

/// `:erlang.term_to_binary/1`.
pub fn encode(term: &Term<'_>) -> Vec<u8> {
    let mut out = vec![131];
    write(&mut out, term);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_term_to_binary() {
        // :erlang.term_to_binary([{nil, "a", nil}, {2, nil, "x"}]) on OTP 29.
        let term = Term::List(vec![
            Term::Tuple(vec![Term::NIL, Term::Binary("a"), Term::NIL]),
            Term::Tuple(vec![Term::Int(2), Term::NIL, Term::Binary("x")]),
        ]);
        assert_eq!(
            encode(&term),
            [
                131, 108, 0, 0, 0, 2, 104, 3, 119, 3, 110, 105, 108, 109, 0, 0, 0, 1, 97, 119, 3,
                110, 105, 108, 104, 3, 97, 2, 119, 3, 110, 105, 108, 109, 0, 0, 0, 1, 120, 106
            ]
        );
        assert_eq!(encode(&Term::List(vec![])), [131, 106]);
        assert_eq!(encode(&Term::Int(-1)), [131, 98, 255, 255, 255, 255]);
        assert_eq!(
            encode(&Term::Int(1 << 40)),
            [131, 110, 6, 0, 0, 0, 0, 0, 0, 1]
        );
    }
}
