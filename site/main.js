import init, { decode_to_json, encode_from_json } from "./pkg/blue_marshal_wasm.js";

const fileInput = document.getElementById("file-input");
const jsonArea = document.getElementById("json");
const generateButton = document.getElementById("generate");
const statusEl = document.getElementById("status");

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}

await init();
setStatus("Ready - choose a file to decode.");

fileInput.addEventListener("change", async () => {
  const file = fileInput.files[0];
  if (!file) return;

  jsonArea.value = "";
  generateButton.disabled = true;

  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const json = decode_to_json(bytes);
    jsonArea.value = json;
    generateButton.disabled = false;
    setStatus(`Decoded "${file.name}" (${bytes.length} bytes).`);
  } catch (err) {
    setStatus(`Failed to decode "${file.name}": ${err}`, true);
  }
});

generateButton.addEventListener("click", () => {
  try {
    const bytes = encode_from_json(jsonArea.value);
    const blob = new Blob([bytes], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "config.dat";
    a.click();
    URL.revokeObjectURL(url);
    setStatus(`Generated configuration file (${bytes.length} bytes).`);
  } catch (err) {
    setStatus(`Failed to generate configuration file: ${err}`, true);
  }
});
