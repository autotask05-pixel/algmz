log.info("starting Algmz Lua runtime")

local helpers = require("ui_helpers")
local greeting = cas.read_text("assets/hello.txt")
log.info(greeting)

local rows = {}

for _, entry in ipairs(cas.list()) do
  log.info(("cas alias=%s hash=%s size=%d"):format(entry.alias, entry.hash, entry.size))
  rows[#rows + 1] = ("<tr><td>%s</td><td><code>%s</code></td><td>%d</td></tr>")
    :format(helpers.escape_html(entry.alias), entry.hash, entry.size)
end

-- Place Bytecode Alliance component-model .wasm files under
-- src-tauri/cas-seed/components and validate them from Lua:
--
-- local ok = wasm.validate_component("components/example.wasm")
-- log.info("component valid: " .. tostring(ok))
--
-- UI components can follow the algmz:runtime/ui-component WIT world and export
-- render: func() -> string. Then Lua can use:
--
-- local html = wasm.render("components/ui.wasm", "render")
-- ui.set_html(html)

local html = [[
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Algmz</title>
    <style>
      :root {
        background: #f7f8f4;
        color: #17201b;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }

      body {
        margin: 0;
      }

      main {
        width: min(960px, calc(100vw - 32px));
        margin: 0 auto;
        padding: 40px 0;
      }

      h1 {
        font-size: clamp(2rem, 8vw, 4rem);
        line-height: 1;
        margin: 0 0 12px;
      }

      p {
        color: #42534a;
        margin: 0 0 28px;
      }

      table {
        border-collapse: collapse;
        width: 100%;
      }

      th,
      td {
        border-bottom: 1px solid #cfd8d1;
        padding: 10px 8px;
        text-align: left;
        vertical-align: top;
      }

      code {
        font-size: 0.78rem;
        overflow-wrap: anywhere;
      }
    </style>
  </head>
  <body>
    <main>
      <h1>Algmz Runtime</h1>
      <p>__GREETING__</p>
      <table>
        <thead>
          <tr>
            <th>Alias</th>
            <th>BLAKE3</th>
            <th>Bytes</th>
          </tr>
        </thead>
        <tbody>
          __ROWS__
        </tbody>
      </table>
    </main>
  </body>
</html>
]]

html = html:gsub("__GREETING__", helpers.escape_html(greeting))
html = html:gsub("__ROWS__", table.concat(rows, "\n"))

ui.set_html(html)
