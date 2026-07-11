use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use mlua::{Function, Lua, Table, Value};
use wasmtime::{
    component::{Component, Linker, Val},
    Config, Engine, Store,
};

use crate::cas::CasStore;

const MAIN_LUA: &str = "scripts/main.lua";

#[derive(Clone)]
struct UiState {
    file: PathBuf,
    html: Arc<Mutex<Option<String>>>,
}

pub fn run_main(store: Arc<CasStore>, ui_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(ui_dir)
        .with_context(|| format!("failed to create UI directory {}", ui_dir.display()))?;

    let ui_file = ui_dir.join("index.html");
    let ui_state = UiState {
        file: ui_file.clone(),
        html: Arc::new(Mutex::new(None)),
    };

    let source = store.read_text(MAIN_LUA)?;
    let lua = Lua::new();

    install_globals(&lua, store, ui_state.clone())?;

    lua.load(&source)
        .set_name(MAIN_LUA)
        .exec()
        .context("failed to execute scripts/main.lua")?;

    let html = ui_state
        .html
        .lock()
        .map_err(|error| anyhow::anyhow!("UI state lock failed: {error}"))?
        .clone()
        .unwrap_or_else(default_html);

    fs::write(&ui_state.file, html)
        .with_context(|| format!("failed to write generated UI {}", ui_state.file.display()))?;

    Ok(ui_file)
}

fn install_globals(lua: &Lua, store: Arc<CasStore>, ui_state: UiState) -> Result<()> {
    let globals = lua.globals();
    globals.set("cas", cas_table(lua, Arc::clone(&store))?)?;
    globals.set("lua", lua_table(lua, Arc::clone(&store))?)?;
    install_cas_require(lua, Arc::clone(&store))?;
    globals.set("wasm", wasm_table(lua, store)?)?;
    globals.set("ui", ui_table(lua, ui_state)?)?;
    globals.set("log", log_table(lua)?)?;
    Ok(())
}

fn lua_table(lua: &Lua, store: Arc<CasStore>) -> Result<Table> {
    let table = lua.create_table()?;

    {
        let store = Arc::clone(&store);
        table.set(
            "load",
            lua.create_function(move |lua, id: String| load_lua_chunk(lua, &store, &id))?,
        )?;
    }

    table.set(
        "run",
        lua.create_function(move |lua, id: String| {
            let chunk = load_lua_chunk(lua, &store, &id)?;
            chunk.call::<Value>(())
        })?,
    )?;

    Ok(table)
}

fn install_cas_require(lua: &Lua, store: Arc<CasStore>) -> Result<()> {
    let globals = lua.globals();
    let original_require: Function = globals.get("require")?;

    globals.set(
        "require",
        lua.create_function(move |lua, module: String| {
            let alias = module_to_alias(&module);

            match load_lua_chunk(lua, &store, &alias) {
                Ok(chunk) => {
                    let value = chunk.call::<Value>(())?;
                    Ok(if matches!(value, Value::Nil) {
                        Value::Boolean(true)
                    } else {
                        value
                    })
                }
                Err(_) => original_require.call::<Value>(module),
            }
        })?,
    )?;

    Ok(())
}

fn load_lua_chunk(lua: &Lua, store: &CasStore, id: &str) -> mlua::Result<Function> {
    let source = store.read_text(id).map_err(mlua::Error::external)?;
    lua.load(&source).set_name(id).into_function()
}

fn module_to_alias(module: &str) -> String {
    let path = module.replace('.', "/");
    if path.ends_with(".lua") || path.starts_with("scripts/") {
        path
    } else {
        format!("scripts/{path}.lua")
    }
}

fn cas_table(lua: &Lua, store: Arc<CasStore>) -> Result<Table> {
    let table = lua.create_table()?;

    {
        let store = Arc::clone(&store);
        table.set(
            "hash",
            lua.create_function(move |_, id: String| {
                store.resolve(&id).map_err(mlua::Error::external)
            })?,
        )?;
    }

    {
        let store = Arc::clone(&store);
        table.set(
            "path",
            lua.create_function(move |_, id: String| {
                store
                    .object_file(&id)
                    .map(|path| path.to_string_lossy().to_string())
                    .map_err(mlua::Error::external)
            })?,
        )?;
    }

    {
        let store = Arc::clone(&store);
        table.set(
            "read_text",
            lua.create_function(move |_, id: String| {
                store.read_text(&id).map_err(mlua::Error::external)
            })?,
        )?;
    }

    {
        let store = Arc::clone(&store);
        table.set(
            "read_bytes",
            lua.create_function(move |lua, id: String| {
                let bytes = store.read(&id).map_err(mlua::Error::external)?;
                lua.create_string(&bytes)
            })?,
        )?;
    }

    table.set(
        "list",
        lua.create_function(move |lua, ()| {
            let entries = store.list().map_err(mlua::Error::external)?;
            let lua_entries = lua.create_table()?;

            for (index, entry) in entries.into_iter().enumerate() {
                let row = lua.create_table()?;
                row.set("alias", entry.alias)?;
                row.set("hash", entry.hash)?;
                row.set("size", entry.size)?;
                lua_entries.set(index + 1, row)?;
            }

            Ok(lua_entries)
        })?,
    )?;

    Ok(table)
}

fn wasm_table(lua: &Lua, store: Arc<CasStore>) -> Result<Table> {
    let table = lua.create_table()?;

    {
        let store = Arc::clone(&store);
        table.set(
            "component_path",
            lua.create_function(move |_, id: String| {
                store
                    .object_file(&id)
                    .map(|path| path.to_string_lossy().to_string())
                    .map_err(mlua::Error::external)
            })?,
        )?;
    }

    {
        let store = Arc::clone(&store);
        table.set(
            "validate_component",
            lua.create_function(move |_, id: String| {
                let path = store.object_file(&id).map_err(mlua::Error::external)?;
                let mut config = Config::new();
                config.wasm_component_model(true);
                let engine = Engine::new(&config).map_err(mlua::Error::external)?;
                Component::from_file(&engine, &path).map_err(mlua::Error::external)?;
                Ok(true)
            })?,
        )?;
    }

    {
        let store = Arc::clone(&store);
        table.set(
            "render",
            lua.create_function(move |_, (id, export): (String, Option<String>)| {
                let export = export.unwrap_or_else(|| "render".to_owned());
                render_component(&store, &id, &export).map_err(mlua::Error::external)
            })?,
        )?;
    }

    Ok(table)
}

fn render_component(store: &CasStore, id: &str, export: &str) -> Result<String> {
    let path = store.object_file(id)?;
    let mut config = Config::new();
    config.wasm_component_model(true);

    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, &path)?;
    let linker = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component)?;
    let func = instance
        .get_func(&mut store, export)
        .with_context(|| format!("component export not found or not a function: {export}"))?;
    let mut results = [Val::String(String::new())];

    func.call(&mut store, &[], &mut results)?;
    func.post_return(&mut store)?;

    match results.into_iter().next() {
        Some(Val::String(html)) => Ok(html),
        _ => anyhow::bail!("component export {export} did not return a string"),
    }
}

fn log_table(lua: &Lua) -> Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "info",
        lua.create_function(|_, message: String| {
            println!("[lua] {message}");
            Ok(())
        })?,
    )?;

    table.set(
        "error",
        lua.create_function(|_, message: String| {
            eprintln!("[lua] {message}");
            Ok(())
        })?,
    )?;

    Ok(table)
}

fn ui_table(lua: &Lua, state: UiState) -> Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "set_html",
        lua.create_function(move |_, html: String| {
            let mut current = state
                .html
                .lock()
                .map_err(|error| mlua::Error::external(error.to_string()))?;
            *current = Some(html);
            Ok(())
        })?,
    )?;

    Ok(table)
}

fn default_html() -> String {
    r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Algmz</title>
  </head>
  <body>
    <main>
      <h1>Algmz Runtime</h1>
      <p>main.lua did not call ui.set_html.</p>
    </main>
  </body>
</html>
"#
    .to_owned()
}
