"use strict";
//
// Arm A's workload: the same programs as the guest, built through the DOM
// bindings the script runtime already exposes.
//
// This file transcribes `workload/src/lib.rs` and `workload/src/scenario.rs`.
// It transcribes the *generators*, not each scenario, so adding a
// configuration to the sweep adds no JavaScript at all, and the identity
// assertion covers every configuration rather than one hand-written case.
//
// There is no way to link a Rust crate into a JavaScript heap, which is why
// this is a transcription and not a shared definition. That is itself part of
// what the boundary costs. `tests/arms_agree.rs` fails if it drifts.
//
// Every string literal below crosses into the host on every call that uses it.
// There is no interned tier here: `createElement("div")` copies the three bytes
// of "div" once per element. That is not a fault in this file, and writing it
// more cleverly would not change it, because the copy happens inside the
// binding rather than in the caller. It is the measurement.

// ---------------------------------------------------------------------------
// The generator. Must match `workload::Rng` bit for bit.
//
// xorshift32 was chosen precisely so this transcription can be exact.
// JavaScript has no 64-bit integer arithmetic outside BigInt, so a 64-bit
// generator would have to be emulated here, and an emulation that is subtly
// wrong produces a different tree, a different byte count, and a difference
// between the arms that looks like a finding.
//
// `>>> 0` after each step returns the value to unsigned. `<<` and `^` operate
// on signed int32 in JavaScript, which has the same bit pattern as the u32
// arithmetic in Rust; only the interpretation differs, and `>>> 0` fixes that
// before the next step reads it.
// ---------------------------------------------------------------------------

var SEED = 0; // overwritten by the harness, from workload::SEED
var __state = SEED;

function nextU32() {
  var x = __state;
  x ^= x << 13;
  x >>>= 0;
  x ^= x >>> 17;
  x >>>= 0;
  x ^= x << 5;
  x >>>= 0;
  __state = x;
  return x;
}

// Upstream's `Math.round(Math.random() * 1000) % max`, with the draw replaced
// and the biased fold deliberately kept. See `workload::Rng::index`.
function index(max) {
  return nextU32() % 1001 % max;
}

// ---------------------------------------------------------------------------
// The upstream row workload. Verbatim lists from upstream Main.js.
// ---------------------------------------------------------------------------

var ADJECTIVES = [
  "pretty", "large", "big", "small", "tall", "short", "long", "handsome",
  "plain", "quaint", "clean", "elegant", "easy", "angry", "crazy", "helpful",
  "mushy", "odd", "unsightly", "adorable", "important", "inexpensive", "cheap",
  "expensive", "fancy",
];
// "brown" really is in here twice, at index 5 and index 7.
var COLOURS = [
  "red", "yellow", "blue", "green", "pink", "brown", "purple", "brown",
  "white", "black", "orange",
];
var NOUNS = [
  "table", "chair", "house", "bbq", "desk", "car", "pony", "cookie",
  "sandwich", "burger", "pizza", "mouse", "keyboard",
];

var nextId = 1;

// The three draws happen in this order and the arms depend on it: the
// generator is a sequence, so drawing colour before adjective would give every
// subsequent row a different label. JavaScript evaluates `a + b + c` left to
// right, which is what makes the transcription faithful.
function nextRow() {
  var id = nextId++;
  var label =
    ADJECTIVES[index(ADJECTIVES.length)] +
    " " +
    COLOURS[index(COLOURS.length)] +
    " " +
    NOUNS[index(NOUNS.length)];
  return { id: id, label: label };
}

var tbody = null;
var labels = [];

function prepare() {
  var table = document.createElement("table");
  tbody = document.createElement("tbody");
  table.appendChild(tbody);
  document.body.appendChild(table);
  labels = [];
  __state = SEED;
  nextId = 1;
}

function createRows(count) {
  for (var i = 0; i < count; i++) {
    var row = nextRow();

    var tr = document.createElement("tr");

    var idCell = document.createElement("td");
    idCell.setAttribute("class", "col-md-1");
    idCell.appendChild(document.createTextNode(String(row.id)));
    tr.appendChild(idCell);

    var labelCell = document.createElement("td");
    labelCell.setAttribute("class", "col-md-4");
    var anchor = document.createElement("a");
    anchor.appendChild(document.createTextNode(row.label));
    labelCell.appendChild(anchor);
    tr.appendChild(labelCell);

    var removeCell = document.createElement("td");
    removeCell.setAttribute("class", "col-md-1");
    var removeAnchor = document.createElement("a");
    var glyph = document.createElement("span");
    glyph.setAttribute("class", "glyphicon glyphicon-remove");
    glyph.setAttribute("aria-hidden", "true");
    removeAnchor.appendChild(glyph);
    removeCell.appendChild(removeAnchor);
    tr.appendChild(removeCell);

    var spacer = document.createElement("td");
    spacer.setAttribute("class", "col-md-6");
    tr.appendChild(spacer);

    tbody.appendChild(tr);
    labels.push({ node: anchor, label: row.label });
  }
}

function updateEveryTenth() {
  for (var i = 0; i < labels.length; i += 10) {
    labels[i].label += " !!!";
    labels[i].node.textContent = labels[i].label;
  }
}

// ---------------------------------------------------------------------------
// The sweep. Transcribes `workload::scenario`.
// ---------------------------------------------------------------------------

// Fixed pool, so this is a literal array rather than a generation rule that
// could drift from the Rust one.
var UTILITIES = [
  "flex", "items-center", "justify-between", "px-4", "py-2", "rounded-lg",
  "border", "border-slate-200", "bg-white", "text-sm", "font-medium",
  "shadow-sm", "hover:bg-slate-50", "focus:outline-none", "transition-colors",
  "duration-150", "gap-2", "w-full", "max-w-md", "truncate", "cursor-pointer",
  "select-none", "leading-tight", "tracking-wide",
];
var UTILITIES_PER_NAME = 6;
var TAG = "div";
var BATCH = 16;
var MOUNT = -1; // usize::MAX in Rust; a sentinel either way.

// The `c{index}` prefix is not decoration: six tokens from a pool of
// twenty-four collide, and two "distinct" vocabulary entries would be the same
// string. The distinct-name count is the denominator of the repetition ratio
// this sweep exists to measure.
function classValue(idx, names) {
  if (names === 0) {
    return "c" + idx;
  }
  var value = "c" + idx;
  for (var slot = 0; slot < UTILITIES_PER_NAME; slot++) {
    value += " " + UTILITIES[(idx * 7 + slot * 11) % UTILITIES.length];
  }
  return value;
}

// Content, not vocabulary: no design can intern it, which is what makes the
// chattiness axis a clean per-call versus per-byte test.
function textChunk(idx) {
  return "n" + idx + " ";
}

function vocabularyIndexFor(config, element) {
  // 1 === UniquePerElement.
  if (config.vocabularyKind === 1) {
    return element;
  }
  return element % Math.max(config.vocabularySize, 1);
}

// Must match `Program::build` step for step, including which branches draw
// from the generator and how many times. A draw taken in a different order
// gives a different tree from element one onwards.
function buildProgram(config) {
  __state = SEED;
  var steps = [];
  var i;

  for (var element = 0; element < config.elements; element++) {
    steps.push({ k: 0 });
    steps.push({ k: 1, v: vocabularyIndexFor(config, element) });
    steps.push({ k: 5 });
    steps.push({ k: 6, t: element });

    var parent;
    if (config.shape === 0) {
      // Deep: a single chain. No draws.
      parent = element === 0 ? MOUNT : element - 1;
    } else if (config.shape === 1) {
      // Wide: one fan. No draws.
      parent = MOUNT;
    } else {
      // Realistic. Element 0 takes no draw; every other element takes one,
      // and the branching case takes a second.
      if (element === 0) {
        parent = MOUNT;
      } else if (index(2) === 0) {
        parent = element - 1;
      } else {
        var window = Math.min(element, 32);
        parent = element - 1 - index(window);
      }
    }
    steps.push({ k: 2, p: parent, c: element });
  }

  if (config.chattiness === 0) {
    for (i = 0; i < config.elements; i++) {
      steps.push({ k: 3, t: i, s: textChunk(i) });
    }
  } else {
    i = 0;
    while (i < config.elements) {
      var end = Math.min(i + BATCH, config.elements);
      var text = "";
      for (var chunk = i; chunk < end; chunk++) {
        text += textChunk(chunk);
      }
      steps.push({ k: 3, t: i, s: text });
      i = end;
    }
  }

  if (config.listeners > 0) {
    var stride = Math.max(Math.floor(config.elements / config.listeners), 1);
    var target = 0;
    var attached = 0;
    while (target < config.elements && attached < config.listeners) {
      steps.push({ k: 4, t: target });
      target += stride;
      attached += 1;
    }
  }

  return steps;
}

var program = [];
var nodes = [];
// Empty text nodes, one per element, created during the build so the text phase
// rewrites a text node in place rather than replacing an element's children and
// destroying the tree the shape axis just built.
var texts = [];
// Every distinct class value, built once here. A lookup, not a format:
// rendering the value per step put a string formatter inside the measured
// window and swamped the bytes it was supposed to be measuring.
var vocabulary = [];
var sweepConfig = null;
var fired = 0;

function onClick() {
  fired += 1;
}

// Expand the configuration. Untimed and uncounted: this allocates every string
// the run will send, and doing it inside the measured call would put a string
// formatter into a measurement of a binding.
function sweepPrepare(vocabularyKind, vocabularySize, names, chattiness, shape, elements, listeners) {
  sweepConfig = {
    vocabularyKind: vocabularyKind,
    vocabularySize: vocabularySize,
    names: names,
    chattiness: chattiness,
    shape: shape,
    elements: elements,
    listeners: listeners,
  };
  program = buildProgram(sweepConfig);

  var distinct = vocabularyKind === 1 ? elements : Math.min(Math.max(vocabularySize, 1), elements);
  vocabulary = [];
  for (var i = 0; i < distinct; i++) {
    vocabulary.push(classValue(i, names));
  }

  nodes = [];
  texts = [];
  fired = 0;
}

// Execute the program. This is the measured operation.
function sweepBuild() {
  for (var i = 0; i < program.length; i++) {
    var step = program[i];
    switch (step.k) {
      case 0:
        nodes.push(document.createElement(TAG));
        break;
      case 1:
        nodes[nodes.length - 1].setAttribute("class", vocabulary[step.v]);
        break;
      case 2:
        if (step.p === MOUNT) {
          document.body.appendChild(nodes[step.c]);
        } else {
          nodes[step.p].appendChild(nodes[step.c]);
        }
        break;
      case 3:
        texts[step.t].textContent = step.s;
        break;
      case 4:
        nodes[step.t].addEventListener("click", onClick);
        break;
      case 5:
        // Empty: zero bytes across the boundary. The text phase fills it later.
        texts.push(document.createTextNode(""));
        break;
      case 6:
        nodes[step.t].appendChild(texts[step.t]);
        break;
    }
  }
}

// Write how many handlers ran into the first element's text node, so the arms
// can be compared after a dispatch storm. Without this the storm leaves no
// trace and two arms that dispatched different numbers of events would still
// serialise identically: the identity assertion would pass while the event axis
// measured nothing.
function sweepSettle() {
  if (texts.length > 0) {
    texts[0].textContent = "fired:" + fired;
  }
}
