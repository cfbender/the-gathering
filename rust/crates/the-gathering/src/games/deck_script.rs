//! Member deck chooser scripts: sandboxed Lua 5.4 that turns each playable deck into a
//! selection weight.
//!
//! A script defines `weight(deck, ctx)` and returns a number (optionally followed by a short
//! note shown next to the pick). Every run gets a fresh Lua state with only the `string`,
//! `table`, `math`, and `utf8` libraries and the safe parts of the base library: no files,
//! `os`, `require`, `load`, or `debug`. Memory, instructions, and wall-clock time are capped,
//! and `print` writes to a log the preview shows. The limits are checked between Lua
//! instructions, so `deck_script_prelude.lua` replaces the library functions that could
//! otherwise run past them in C (Lua patterns among them; `string.find` is plain-text only).
//! `docs/deck-chooser-scripts.md` documents the `deck` and `ctx` tables.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{
    Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt, LuaString, MultiValue, StdLib, Table,
    VmState, serde::SerializeOptions,
};
use serde::Serialize;
use serde_json::Value;

/// The longest script a member can save.
pub const MAX_SCRIPT_BYTES: usize = 64 * 1024;
const MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const INSTRUCTION_LIMIT: u64 = 50_000_000;
const HOOK_INTERVAL: u32 = 1_000;
const TIME_LIMIT: Duration = Duration::from_millis(500);
/// The longest text `string.find` searches, so each call stays far below the time limit.
const MAX_FIND_BYTES: usize = 16 * 1024;
const MAX_LOG_LINES: usize = 200;
const MAX_LOG_CHARS: usize = 500;
const MAX_NOTE_CHARS: usize = 80;
/// Error positions read `script:12: message`.
const CHUNK_NAME: &str = "=script";
const UNSAFE_GLOBALS: [&str; 5] = ["dofile", "loadfile", "load", "collectgarbage", "require"];

/// Why a script failed, with the line it failed on when Lua reported one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScriptError {
    /// What went wrong, without the `script:LINE:` prefix.
    pub message: String,
    /// 1-based line in the script.
    pub line: Option<i64>,
}

impl ScriptError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }

    fn from_lua(error: &mlua::Error) -> Self {
        let message = describe(error);
        let message = message
            .split("\nstack traceback:")
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        // `script:12: attempt to ...`, possibly behind a `runtime error: ` prefix.
        let located = message
            .find("script:")
            .and_then(|start| message.get(start + "script:".len()..))
            .and_then(|rest| {
                let (line, text) = rest.split_once(':')?;
                Some((line.parse::<i64>().ok()?, text.trim().to_owned()))
            });
        match located {
            Some((line, text)) => Self {
                message: text,
                line: Some(line),
            },
            None => Self::new(message),
        }
    }
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

fn describe(error: &mlua::Error) -> String {
    match error {
        mlua::Error::SyntaxError { message, .. } | mlua::Error::RuntimeError(message) => {
            message.clone()
        }
        mlua::Error::MemoryError(_) => format!(
            "the script ran out of memory (the limit is {} MB)",
            MEMORY_LIMIT_BYTES / 1024 / 1024
        ),
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
            describe(cause)
        }
        other => other.to_string(),
    }
}

/// One deck's weight from the script.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Weighted {
    /// Zero or more; zero leaves the deck out.
    pub weight: f64,
    /// The script's optional second return value.
    pub note: Option<String>,
}

/// The outcome of a run and everything the script printed.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// One weight per `ctx.decks` entry, in order, or why the script failed.
    pub result: Result<Vec<Weighted>, ScriptError>,
    /// `print` output, one entry per call.
    pub logs: Vec<String>,
}

fn limit_error() -> mlua::Error {
    mlua::Error::RuntimeError(format!(
        "the script ran too long (limits: {} million instructions, {} ms)",
        INSTRUCTION_LIMIT / 1_000_000,
        TIME_LIMIT.as_millis()
    ))
}

/// A fresh sandboxed state with limits armed and `print` captured into `logs`.
fn sandbox(logs: &Rc<RefCell<Vec<String>>>) -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::new(),
    )?;
    lua.set_memory_limit(MEMORY_LIMIT_BYTES)?;
    let instructions = Cell::new(0_u64);
    let deadline = Instant::now() + TIME_LIMIT;
    // Once set, the prelude's `pcall` and `xpcall` re-raise the limit error.
    let tripped = Rc::new(Cell::new(false));
    let hook_tripped = Rc::clone(&tripped);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
        move |_, _| {
            instructions.set(instructions.get().saturating_add(u64::from(HOOK_INTERVAL)));
            if hook_tripped.get()
                || instructions.get() > INSTRUCTION_LIMIT
                || Instant::now() > deadline
            {
                hook_tripped.set(true);
                return Err(limit_error());
            }
            Ok(VmState::Continue)
        },
    )?;
    let globals = lua.globals();
    for name in UNSAFE_GLOBALS {
        globals.raw_set(name, mlua::Nil)?;
    }
    globals.get::<Table>("string")?.raw_set("dump", mlua::Nil)?;
    let sink = Rc::clone(logs);
    let print = lua.create_function(move |lua, values: MultiValue| {
        let mut sink = sink.borrow_mut();
        if sink.len() >= MAX_LOG_LINES {
            return Ok(());
        }
        let tostring: Function = lua.globals().get("tostring")?;
        let parts = values
            .into_iter()
            .map(|value| tostring.call::<String>(value))
            .collect::<mlua::Result<Vec<_>>>()?;
        sink.push(truncate(&parts.join("\t"), MAX_LOG_CHARS));
        Ok(())
    })?;
    globals.raw_set("print", print)?;
    let guard = lua.create_function(move |_, results: MultiValue| {
        if tripped.get() {
            Err(limit_error())
        } else {
            Ok(results)
        }
    })?;
    let find = lua.create_function(plain_find)?;
    lua.load(include_str!("deck_script_prelude.lua"))
        .set_name("=sandbox")
        .call::<()>((guard, find))?;
    Ok(lua)
}

/// `string.find(s, text [, init])`, searching for `text` literally (in linear time) instead
/// of as a pattern.
fn plain_find(
    _: &Lua,
    (haystack, needle, init): (LuaString, LuaString, Option<i64>),
) -> mlua::Result<MultiValue> {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    if haystack.len() > MAX_FIND_BYTES || needle.len() > MAX_FIND_BYTES {
        return Err(mlua::Error::RuntimeError(format!(
            "string.find searches text up to {} KB",
            MAX_FIND_BYTES / 1024
        )));
    }
    let len = i64::try_from(haystack.len()).unwrap_or(i64::MAX);
    // Lua's rules: 1-based, negative counts back from the end, clamped to the start.
    let init = match init.unwrap_or(1) {
        init if init > 0 => init,
        0 => 1,
        init => len.saturating_add(init).saturating_add(1).max(1),
    };
    let found = usize::try_from(init - 1)
        .ok()
        .and_then(|start| Some(start + memchr::memmem::find(haystack.get(start..)?, &needle)?));
    Ok(MultiValue::from_vec(match found {
        Some(start) => {
            let first = i64::try_from(start).unwrap_or(i64::MAX).saturating_add(1);
            let needle_len = i64::try_from(needle.len()).unwrap_or(i64::MAX);
            vec![
                mlua::Value::Integer(first),
                mlua::Value::Integer(first.saturating_add(needle_len) - 1),
            ]
        }
        None => vec![mlua::Value::Nil],
    }))
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(max).collect();
        cut.push('…');
        cut
    }
}

/// Runs the script's top level and returns its `weight` function.
fn load_weight(lua: &Lua, script: &str) -> Result<Function, ScriptError> {
    lua.load(script)
        .set_name(CHUNK_NAME)
        .set_mode(mlua::chunk::ChunkMode::Text)
        .exec()
        .map_err(|error| ScriptError::from_lua(&error))?;
    match lua.globals().get::<mlua::Value>("weight") {
        Ok(mlua::Value::Function(weight)) => Ok(weight),
        _ => Err(ScriptError::new(
            "the script must define a function named weight(deck, ctx)",
        )),
    }
}

/// Checks that `script` compiles, its top level runs, and it defines `weight`.
pub fn check(script: &str) -> Result<(), ScriptError> {
    if script.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::new(format!(
            "the script is longer than {} KB",
            MAX_SCRIPT_BYTES / 1024
        )));
    }
    let logs = Rc::new(RefCell::new(Vec::new()));
    let lua = sandbox(&logs).map_err(|error| ScriptError::from_lua(&error))?;
    load_weight(&lua, script).map(|_| ())
}

fn weigh(lua: &Lua, script: &str, context: &Value) -> Result<Vec<Weighted>, ScriptError> {
    let weight = load_weight(lua, script)?;
    let options = SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false)
        .set_array_metatable(false);
    let lua_error = |error: mlua::Error| ScriptError::from_lua(&error);
    let ctx = lua.to_value_with(context, options).map_err(lua_error)?;
    let mlua::Value::Table(ctx) = ctx else {
        return Err(ScriptError::new("the deck chooser context is not a table"));
    };
    let decks: Table = ctx.get("decks").map_err(lua_error)?;
    let mut weights = Vec::new();
    for deck in decks.sequence_values::<Table>() {
        let deck = deck.map_err(lua_error)?;
        let name: String = deck.get("name").unwrap_or_default();
        let (value, note): (mlua::Value, mlua::Value) =
            weight.call((deck, ctx.clone())).map_err(lua_error)?;
        let number = match value {
            mlua::Value::Integer(number) => i32::try_from(number).map_or_else(
                |_| number.to_string().parse::<f64>().unwrap_or(f64::INFINITY),
                f64::from,
            ),
            mlua::Value::Number(number) => number,
            other => {
                return Err(ScriptError::new(format!(
                    "weight() must return a number for {name}, got {}",
                    other.type_name()
                )));
            }
        };
        if !number.is_finite() || number < 0.0 {
            return Err(ScriptError::new(format!(
                "weight() must return a finite number zero or above for {name}, got {number}"
            )));
        }
        let note = match note {
            mlua::Value::Nil => None,
            other => Some(truncate(
                &other.to_string().map_err(lua_error)?,
                MAX_NOTE_CHARS,
            )),
        };
        weights.push(Weighted {
            weight: number,
            note,
        });
    }
    Ok(weights)
}

/// Calls `weight(deck, ctx)` for each entry of `context.decks`, passing `context` as `ctx`.
///
/// Blocking: run it off the async runtime.
pub fn run(script: &str, context: &Value) -> Run {
    let logs = Rc::new(RefCell::new(Vec::new()));
    let result = sandbox(&logs)
        .map_err(|error| ScriptError::from_lua(&error))
        .and_then(|lua| weigh(&lua, script, context));
    let logs = logs.take();
    Run { result, logs }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn context() -> Value {
        json!({"games_today": 2, "decks": [
            {"id": 1, "name": "Krenko", "plays": 4, "colors": ["R"], "last_game": null},
            {"id": 2, "name": "Atraxa", "plays": 0, "colors": ["W", "U", "B", "G"]},
        ]})
    }

    fn weights(script: &str) -> Result<Vec<f64>, ScriptError> {
        run(script, &context()).result.map(|weights| {
            weights
                .into_iter()
                .map(|weighted| weighted.weight)
                .collect()
        })
    }

    #[test]
    fn weighs_each_deck_with_its_table_and_the_context() {
        let script = r#"
            function weight(deck, ctx)
              assert(deck.last_game == nil, "JSON null is nil")
              return (deck.plays + 1) * ctx.games_today * #deck.colors, deck.name .. "!"
            end
        "#;
        let run = run(script, &context());
        assert_eq!(
            run.result.unwrap(),
            vec![
                Weighted {
                    weight: 10.0,
                    note: Some("Krenko!".into())
                },
                Weighted {
                    weight: 8.0,
                    note: Some("Atraxa!".into())
                },
            ]
        );
    }

    #[test]
    fn rejects_weights_that_are_not_finite_non_negative_numbers() {
        for (body, expected) in [
            ("return -1", "zero or above for Krenko, got -1"),
            ("return 0/0", "zero or above for Krenko, got NaN"),
            ("return math.huge", "zero or above for Krenko, got inf"),
            (
                "return 'heavy'",
                "must return a number for Krenko, got string",
            ),
            ("return nil", "must return a number for Krenko, got nil"),
        ] {
            let error = weights(&format!("function weight(deck) {body} end")).unwrap_err();
            assert!(error.message.contains(expected), "{body}: {error}");
        }
        assert_eq!(
            weights("function weight(deck) if deck.id == 2 then return 0 end return 3 end")
                .unwrap(),
            vec![3.0, 0.0]
        );
    }

    #[test]
    fn reports_syntax_and_runtime_errors_with_their_lines() {
        assert_eq!(
            check("function weight(deck)\n  return deck.\nend")
                .unwrap_err()
                .line,
            Some(3)
        );
        let error =
            weights("\nfunction weight(deck)\n  return deck.missing.field\nend").unwrap_err();
        assert_eq!(error.line, Some(3));
        assert!(error.message.contains("attempt to index"), "{error}");
        assert_eq!(
            check("local x = 1").unwrap_err().message,
            "the script must define a function named weight(deck, ctx)"
        );
        let error = weights("function weight() error('boom') end").unwrap_err();
        assert_eq!((error.line, error.message.as_str()), (Some(1), "boom"));
    }

    #[test]
    fn sandbox_hides_files_processes_and_code_loading() {
        for global in [
            "io", "os", "debug", "package", "require", "load", "loadfile", "dofile",
        ] {
            let script =
                format!("assert({global} == nil, '{global}') function weight() return 1 end");
            assert!(check(&script).is_ok(), "{global} is reachable");
        }
        assert!(check("assert(string.dump == nil) function weight() return 1 end").is_ok());
        assert!(check("assert(math.floor(2.5) == 2) function weight() return 1 end").is_ok());
    }

    #[test]
    fn stops_runaway_scripts() {
        let error = check("while true do end function weight() return 1 end").unwrap_err();
        assert!(error.message.contains("ran too long"), "{error}");
        let error = weights("function weight() local t = {} while true do t[#t + 1] = string.rep('x', 1024) end end")
            .unwrap_err();
        assert!(error.message.contains("out of memory"), "{error}");
        assert!(check(&"-".repeat(MAX_SCRIPT_BYTES + 1)).is_err());
    }

    /// Fails with the limit error, well before C code that ignored the limits would return.
    fn assert_stopped(script: &str) {
        let started = Instant::now();
        let error = check(&format!("{script} function weight() return 1 end")).unwrap_err();
        assert!(error.message.contains("ran too long"), "{script}: {error}");
        assert!(started.elapsed() < Duration::from_secs(5), "{script}");
    }

    #[test]
    fn library_functions_cannot_outrun_the_limits() {
        // As a Lua pattern this backtracks for minutes before matching; as plain text it never
        // matches, so the loop runs until the limit.
        assert_stopped(
            "while not string.find(string.rep('a', 30), string.rep('a?', 30) .. string.rep('a', 30)) do end",
        );
        // These loop in C once per repetition or position.
        assert_stopped("local s = string.rep('', 1e10) while true do end");
        assert_stopped("table.move({}, 1, 1e10, 1, {})");
        let huge = "setmetatable({}, {__len = function() return 1e10 end})";
        assert_stopped(&format!("table.insert({huge}, 1, 'x')"));
        assert_stopped(&format!("table.remove({huge}, 1)"));
        // pcall cannot swallow the limit error.
        assert_stopped("for i = 1, 1e6 do pcall(function() while true do end end) end");
        assert_stopped("for i = 1, 1e6 do xpcall(function() while true do end end, print) end");
        // Finalizers run with hooks off.
        let error =
            check("setmetatable({}, {__gc = false}) function weight() return 1 end").unwrap_err();
        assert_eq!(
            (error.line, error.message.as_str()),
            (Some(1), "__gc metamethods are not allowed")
        );
    }

    #[test]
    fn replaced_library_functions_keep_their_behavior() {
        let script = r##"
            local t = {1, 2, 3}
            table.insert(t, 4)
            table.insert(t, 1, 0)
            assert(table.concat(t, ",") == "0,1,2,3,4")
            assert(table.remove(t) == 4 and table.remove(t, 1) == 0)
            assert(table.concat(t, ",") == "1,2,3" and table.remove({}) == nil)
            assert(table.concat(table.move({1, 2, 3}, 1, 3, 2), ",") == "1,1,2,3")
            assert(table.concat(table.move({1, 2, 3}, 2, 3, 1), ",") == "2,3,3")
            assert(table.concat(table.move({1, 2}, 1, 2, 2, {9}), ",") == "9,1,2")
            assert(string.rep("ab", 3, "-") == "ab-ab-ab" and string.rep("", 5) == "")
            local first, last = ("a.b.c"):find(".", 3)
            assert(first == 4 and last == 4, "plain text, not a pattern")
            assert(select("#", string.find("Krenko", "x")) == 1)
            assert(("hello"):find("lo", -2) == 4 and ("abc"):find("", 4) == 4)
            assert(("abc"):find("", 5) == nil and ("abc"):find("a", 0) == 1)
            assert(string.match == nil and string.gmatch == nil and string.gsub == nil)
            assert(pcall(error, "caught") == false and select(2, pcall(math.abs, -2)) == 2)
            function weight() return 1 end
        "##;
        assert_eq!(check(script), Ok(()));
        let error = check("\ntable.insert({}, 5, 1)").unwrap_err();
        assert_eq!(
            (error.line, error.message.as_str()),
            (
                Some(2),
                "bad argument #2 to 'insert' (position out of bounds)"
            )
        );
        let error = check("table.remove('x')").unwrap_err();
        assert_eq!(
            (error.line, error.message.as_str()),
            (
                Some(1),
                "bad argument #1 to 'remove' (table expected, got string)"
            )
        );
        let error = check(&format!(
            "string.find(string.rep('x', {}), 'x')",
            MAX_FIND_BYTES + 1
        ))
        .unwrap_err();
        assert!(error.message.contains("up to 16 KB"), "{error}");
    }

    #[test]
    fn captures_print_output() {
        let run = run(
            "function weight(deck) print(deck.name, deck.plays, nil) return 1 end",
            &context(),
        );
        assert_eq!(run.logs, vec!["Krenko\t4\tnil", "Atraxa\t0\tnil"]);
    }
}
