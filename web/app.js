// esp-hal workshop: RGB LED room.
//
// Topics, below `esp-hal-workshop/<room>/<board>/`:
//   status  board -> page  "online" / "offline"   (retained, "offline" is the board's last will)
//   color   board -> page  "#rrggbb"              (retained)
//   set     page -> board  "#rrggbb"
// `esp-hal-workshop/<room>/all/set` sets every board in the room.
"use strict";

const PREFIX = "esp-hal-workshop";
const COLOR_RE = /^#[0-9a-fA-F]{6}$/;
const ROOM_RE = /^[A-Za-z0-9_-]{1,32}$/;
const PRESETS = ["#ff0000", "#ff8000", "#ffff00", "#00ff00", "#00ffff", "#0000ff", "#ff00ff", "#ffffff", "#000000"];

// Served by the workshop's local broker (plain http)? Then use its WebSocket port.
const servedLocally = location.protocol === "http:" && location.hostname !== "";
const BROKERS = [
  ...(servedLocally ? [{ name: `This computer's broker (${location.hostname})`, url: `ws://${location.hostname}:8083/mqtt` }] : []),
  { name: "HiveMQ (broker.hivemq.com)", url: "wss://broker.hivemq.com:8884/mqtt" },
  { name: "EMQX (broker.emqx.io)", url: "wss://broker.emqx.io:8084/mqtt" },
  { name: "Mosquitto (test.mosquitto.org)", url: "wss://test.mosquitto.org:8081" },
  { name: "Custom URL...", url: "" },
];

const $ = (id) => document.getElementById(id);
const els = {
  form: $("connect-form"), room: $("room"), broker: $("broker"),
  customLabel: $("custom-label"), customUrl: $("custom-url"), connect: $("connect"),
  status: $("status"), allCard: $("all-card"), boards: $("boards"), empty: $("empty"),
  roomHint: $("room-hint"), topicHint: $("topic-hint"), actions: $("actions"),
  showOffline: $("show-offline"), clearOffline: $("clear-offline"),
  template: $("board-template"),
};

let client = null;
let room = null;
/** name -> { status, color, el } */
const boards = new Map();

// ---- Settings: ?room=...&broker=... in the URL, otherwise the last used ones ----
for (const [i, b] of BROKERS.entries()) {
  const option = document.createElement("option");
  option.value = String(i);
  option.textContent = b.name;
  els.broker.append(option);
}
const params = new URLSearchParams(location.search);
els.room.value = params.get("room") || localStorage.getItem("room") || "lobby";
const savedBroker = params.get("broker") || localStorage.getItem("broker");
const savedIndex = BROKERS.findIndex((b) => b.url && b.url === savedBroker);
if (savedIndex >= 0) {
  els.broker.value = String(savedIndex);
} else if (savedBroker) {
  els.broker.value = String(BROKERS.length - 1);
  els.customUrl.value = savedBroker;
}
const updateCustom = () => { els.customLabel.hidden = BROKERS[Number(els.broker.value)].url !== ""; };
els.broker.addEventListener("change", updateCustom);
updateCustom();

// ---- Connecting ----
els.form.addEventListener("submit", (event) => {
  event.preventDefault();
  const newRoom = els.room.value.trim();
  if (!ROOM_RE.test(newRoom)) {
    setStatus("Room: letters, digits, - and _ only", "error");
    return;
  }
  const choice = BROKERS[Number(els.broker.value)];
  const url = choice.url || els.customUrl.value.trim();
  if (!url) {
    setStatus("Enter a broker URL", "error");
    return;
  }
  localStorage.setItem("room", newRoom);
  localStorage.setItem("broker", url);
  history.replaceState(null, "", `?room=${encodeURIComponent(newRoom)}`);
  connect(url, newRoom);
});

function connect(url, newRoom) {
  if (client) client.end(true);
  boards.clear();
  els.boards.replaceChildren();
  room = newRoom;
  els.roomHint.textContent = room;
  els.topicHint.textContent = `${PREFIX}/${room}/#`;
  setStatus(`Connecting to ${url}...`);

  client = mqtt.connect(url, {
    protocolVersion: 4, // MQTT 3.1.1: every broker supports it over WebSockets
    clientId: `esp-hal-web-${Math.random().toString(16).slice(2, 10)}`,
    clean: true,
    reconnectPeriod: 3000,
    connectTimeout: 10_000,
  });
  const thisClient = client;

  client.on("connect", () => {
    setStatus(`Connected, room "${room}"`, "ok");
    thisClient.subscribe([`${PREFIX}/${room}/+/status`, `${PREFIX}/${room}/+/color`]);
    els.allCard.hidden = false;
    render();
  });
  client.on("reconnect", () => setStatus("Reconnecting..."));
  client.on("offline", () => setStatus("Offline, retrying...", "error"));
  client.on("error", (e) => setStatus(`Error: ${e.message}`, "error"));
  client.on("message", (topic, payload) => {
    if (thisClient !== client) return;
    onMessage(topic, payload.toString());
  });
}

function onMessage(topic, text) {
  const parts = topic.split("/");
  if (parts.length !== 4 || parts[0] !== PREFIX || parts[1] !== room) return;
  const [, , name, leaf] = parts;
  if (name === "all") return;

  if (text === "") {
    // An empty retained message clears the board from the room.
    const board = boards.get(name);
    if (board && leaf === "status") {
      board.el.remove();
      boards.delete(name);
      render();
    }
    return;
  }

  const board = boards.get(name) || addBoard(name);
  if (leaf === "status" && (text === "online" || text === "offline")) board.status = text;
  if (leaf === "color" && COLOR_RE.test(text)) board.color = text.toLowerCase();
  updateBoard(board);
  render();
}

// ---- Board cards ----
function addBoard(name) {
  const el = els.template.content.firstElementChild.cloneNode(true);
  el.querySelector(".name").textContent = name; // text only: names come from a public broker
  buildControls(el.querySelector(".controls"), name);
  const board = { name, status: "unknown", color: "#000000", el };
  boards.set(name, board);
  // Keep the cards sorted by name.
  const after = [...boards.keys()].sort().find((n) => n > name);
  els.boards.insertBefore(el, after ? boards.get(after).el : null);
  return board;
}

function updateBoard(board) {
  const { el, status, color } = board;
  el.classList.toggle("offline", status === "offline");
  const dot = el.querySelector(".dot");
  dot.style.background = color;
  dot.style.setProperty("--glow", color === "#000000" ? "transparent" : color);
  el.querySelector(".state").textContent = `${status} · LED ${color}`;
  const picker = el.querySelector("input[type=color]");
  if (document.activeElement !== picker) picker.value = color;
}

function buildControls(container, target) {
  const picker = document.createElement("input");
  picker.type = "color";
  picker.value = "#000000";
  picker.title = "Pick a color";
  // Send while dragging, at most every 100 ms, and once more when released.
  let last = 0;
  picker.addEventListener("input", () => {
    const now = Date.now();
    if (now - last > 100) { last = now; sendColor(target, picker.value); }
  });
  picker.addEventListener("change", () => sendColor(target, picker.value));
  container.append(picker);

  for (const color of PRESETS) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "preset";
    button.style.background = color;
    button.title = color;
    button.addEventListener("click", () => sendColor(target, color));
    container.append(button);
  }
}

function sendColor(target, color) {
  if (!client || !client.connected || !COLOR_RE.test(color)) return;
  client.publish(`${PREFIX}/${room}/${target}/set`, color.toLowerCase());
}

els.allCard.querySelector(".controls").replaceChildren();
buildControls(els.allCard.querySelector(".controls"), "all");

// ---- Offline boards ----
els.showOffline.addEventListener("change", render);
els.clearOffline.addEventListener("click", () => {
  for (const board of boards.values()) {
    if (board.status !== "offline") continue;
    // Replace the retained messages with empty ones: the broker then forgets them.
    for (const leaf of ["status", "color"]) {
      client.publish(`${PREFIX}/${room}/${board.name}/${leaf}`, "", { retain: true });
    }
  }
});

function render() {
  let visible = 0;
  let offline = 0;
  for (const board of boards.values()) {
    const hide = board.status === "offline" && !els.showOffline.checked;
    board.el.hidden = hide;
    if (!hide) visible++;
    if (board.status === "offline") offline++;
  }
  els.empty.hidden = !client || !client.connected || visible > 0;
  els.actions.hidden = offline === 0;
}

function setStatus(text, kind = "") {
  els.status.textContent = text;
  els.status.className = `status ${kind}`;
}

// Join the room straight away when the URL names one.
if (params.get("room")) els.form.requestSubmit();
