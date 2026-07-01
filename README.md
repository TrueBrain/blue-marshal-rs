# Blue Marshal (in Rust)

A Rust port of the [blue.Marshal](https://github.com/carbonengine/blue/blob/release/2.x/src/Marshal.cpp), plus a lossless JSON encoding for easy editing.

It is meant to read/write EVE Online configuration files.

## Installation

```
cargo add blue-marshal
```

The WASM bindings (to integrate blue-marshal in your website) are published separately on npm:

```
npm install @truebrain/blue-marshal
```

## Usage

```rust
let bytes = std::fs::read("some.marshal")?;
let decoded = blue_marshal::decode(&bytes)?;
let json = blue_marshal::to_json(&decoded.value);

let value = blue_marshal::from_json(&json)?;
let reencoded = blue_marshal::encode(&value, &blue_marshal::EncodeOptions::default())?;
```

A small CLI is included:

```
marshal-tool to-json   in.marshal out.json
marshal-tool from-json in.json    out.marshal
```

## Warning

Although at-most care is giving to make it write valid configuration files, no guarantees are given.
Changing anything about the structure or typing of any of the fields/values can result in invalid configuration files.
Use with care.

## JSON encoding

Native JSON types (`null`, `bool`, numbers, arrays) are used wherever the mapping is unambiguous.
Anything that JSON can't natively express is turned into a prefixed string:

| Value                       | JSON form                                          |
|-----------------------------|----------------------------------------------------|
| `None`                      | `null`                                             |
| `Bool`                      | `true` / `false`                                   |
| `Int`                       | plain number                                       |
| `Float` (finite)            | plain number                                       |
| `Float` (NaN/Inf)           | `"float:nan"`, `"float:inf"`, `"float:-inf"`       |
| `Long` (arbitrary precision)| `"long:<decimal>"`                                 |
| unicode string              | `"utf8:<text>"`                                    |
| byte string, valid UTF-8    | `"bytes:<text>"`                                   |
| byte string, not UTF-8      | `"bytes:b64:<base64>"`                             |
| class/type reference        | `"global:<module.name>"`                           |
| list                        | JSON array                                         |
| tuple                       | `{"tuple": [...]}`                                 |
| dict                        | JSON object (see below for keys)                   |
| old-style instance          | `{"instance": {"class": ..., "state": ...}}`       |
| `__reduce__`/`__newobj__`   | `{"reduce": {...}}`                                |
| save/load callback wrapper  | `{"callback": ...}`                                |

JSON object keys must be strings, but marshal dict keys can be any value (ints, tuples, ...).
So **every** dict key is written with an explicit prefix, including the types that are normally bare JSON as values (`"none"`, `"bool:true"`, `"int:5"`, `"float:1.5"`).
Keys that are themselves compound values are wrapped as `"json:<value-as-json-text>"`.

## What's out of scope

This port intentionally does not implement:

- `TY_DBROW` - EVE's row format, which embeds its own recursive
  descriptor+data sub-encoding.
- `TY_WSTREAM` - a nested pre-built `blue.MarshalStream` blob.
- `TY_PICKLE` / `TY_PICKLER` - the generic `cPickle` fallback path.

Decoding a stream containing any of these tags returns `Error::Unsupported`.
The encoder never produces them.

## Shared references

- Decoding resolves `TY_REFERENCE` immediately by inlining a clone of the referenced value, so the returned tree is reference-free.
  This means a container that references itself before it has finished being built (a genuine cycle) can't be represented as an owned tree - the reference resolves to `Value::None` in that case instead of infinitely recursing.
  `Marshal.cpp` already forbids direct self-referencing tuples, and true  cycles do not show up in ordinary EVE configuration data, so this is a  theoretical, not practical, limitation.
- Encoding never tracks shared objects: every repeated value is written out in full, every time.
  `Marshal.cpp` reads this correctly - the `TY_REFERENCE` opcode is a size optimization, not something the format  requires - it just means a stream re-encoded through this crate can be larger than what the original C++ writer would have produced for heavily-shared data.

## String table

The built-in string table (`MARSHAL_STRINGS` in `Marshal.cpp`) is embedded verbatim in `src/strtable.rs` so `TY_STR_TABLE` reads decode correctly.
The writer never emits `TY_STR_TABLE` - it always writes strings as `TY_BUFFER`, which `Marshal.cpp` reads identically.

## WASM demo

`wasm/` is a thin `wasm-bindgen` shim exposing `decode_to_json`/`encode_from_json`, and `site/` is a small static page (upload a config file, view/edit its JSON, regenerate a config file) that loads it.

Build the wasm package, then serve `site/` with any static file server (it can't be opened via `file://` because of module/CORS restrictions on `fetch`-ing the `.wasm`):

```
wasm-pack build wasm --target web --out-dir ../site/pkg
python3 -m http.server -d site 8080
```

Then open `http://localhost:8080`.

## License

MIT, see [LICENSE.md](LICENSE.md).
That file also carries CCP Games' original MIT license for `Marshal.cpp`, which the embedded string table and wire format in this crate are ported from.
