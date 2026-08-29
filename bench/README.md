# dom-boundary-bench

Measures the cost of the boundary between a guest language and a DOM. Not the
speed of frameworks.

There is no framework here and no framework comparison. One workload is built
three different ways against the same document in the same engine, so the only
thing that differs between the arms is the binding, and the numbers describe the
binding.

```bash
cargo run --release
```

Writes `results.json` and prints a summary. The paper table is generated from
that file rather than transcribed from a terminal.

## The headline

**Interning pays above a repetition ratio of R ≈ 40**, where
`R = total name uses / distinct names`. Below that, the native handle-and-atom
binding is *slower* than the JavaScript one building the identical tree.

**The native binding lost at 15 of the 25 swept configurations.** The worst is
`vunique-short-few-real`, where it is **2.1x slower** than JavaScript: one
distinct class value per element, so it spends 2,002 extra crossings to save
15,992 bytes, a break-even of **8 bytes per crossing** against a crossing that
costs one to three hundred.

That is the point of the sweep. If a run ever reports the native binding winning
everywhere, the right conclusion is that the axes are too narrow or the harness
is broken, not that the ABI is perfect. Two of the seven axes cannot be run at
all, and both are places this design is most likely to lose: see
[Axes that cannot be run](#axes-that-cannot-be-run).

## Provenance

The workload comes from
[krausest/js-framework-benchmark](https://github.com/krausest/js-framework-benchmark),
specifically **`frameworks/keyed/vanillajs/src/Main.js`**. Upstream is
Apache-2.0; its `LICENSE` is included in this repository unchanged.

Taken from that file, verbatim:

- the row markup,
- the label shape, `"{adjective} {colour} {noun}"`,
- the three word lists,
- sequential ids from 1 (`this.id++`),
- both operations: `run` (build 1,000 rows) and `update` (`i += 10`, append
  `" !!!"`).

Everything else here is written fresh. Upstream is 198 MB and 185 framework
implementations; it is not forked, vendored, or copied beyond the above.

**Two notes on the lists**, because both look like transcription errors and
neither is:

- `COLOURS` has **11 entries but 10 distinct colours**. `"brown"` appears twice,
  at index 5 and index 7. That is upstream's list. It is kept because removing
  it would change the label distribution and make these numbers incomparable
  with upstream's; it means brown is drawn twice as often as any other colour.
- `NOUNS` has **13 entries, not 25**. The brief that commissioned this repo said
  25. Upstream's list is thirteen words long, and thirteen is what is here.

## The determinism change

**Upstream draws labels with `Math.random()`. This harness does not.**

That is not a refinement, it is a requirement. Every number in the report is a
difference between arms, and a difference is only about the binding if
everything else was held equal. Ten thousand rows of different text is a
different number of bytes across the boundary, so an unseeded draw would not add
noise to the measurement, it would invalidate it.

So the generator is a fixed-seed **xorshift32** and every arm consumes the
identical sequence.

xorshift32 specifically, because arm A's generator has to be written in
JavaScript and JavaScript has no 64-bit integer arithmetic outside `BigInt`. A
64-bit generator would have to be emulated there, and an emulation that is
subtly wrong produces different labels, a different byte count, and a difference
between the arms that looks like a finding. Every step of xorshift32 is a 32-bit
shift or xor, which JavaScript performs exactly.

Upstream's selection is `Math.round(Math.random() * 1000) % max`. The `% max`
fold is biased whenever `max` does not divide 1001, and **the bias is kept**:
correcting it would change which labels appear and how often, and the point of
copying upstream's lists is to stay comparable with upstream's labels. Only the
source of randomness differs.

`tests/arms_agree.rs` asserts the arms produce byte-identical trees, over the
whole serialisation rather than a node count, **at every configuration in the
sweep**. A generator that had drifted by one draw between the arms would still
produce the right number of nodes with the wrong classes on them, and a length
check would pass.

Covering every configuration is what makes the generator safe to extend. Arm A's
JavaScript transcribes the *generator*, not each scenario, so adding a
configuration adds no JavaScript at all, and a transcription bug shows up as a
tree mismatch at whichever configuration first exercises the branch that was got
wrong. The run itself asserts the same thing before it will print anything, so a
report cannot be produced claiming to compare two arms that built different
documents.

## The sweep

The upstream operations are two fixed points. The sweep is a parameterised
generator with five live knobs, run across 25 configurations, and it exists to
find where the atom tier **loses**.

| axis | favourable end | adversarial end | what it moves |
| --- | --- | --- | --- |
| vocabulary | 8 distinct names | one per element | R, from 600 down to 3 |
| name length | `"c7"` (3 bytes) | 70 bytes of Tailwind | what a name costs to intern |
| chattiness | few large writes | many small ones | crossings at constant bytes |
| tree shape | wide fan | deep chain | nothing measurable, as it turns out |
| event rate | no listeners | 500 listeners, 2,000 clicks | delivery rather than data |

Vocabulary and name length are swept as a **full cross product**, because the
worst case for an atom tier is long *and* distinct together: a long name is
expensive to intern and a distinct one never earns the cost back. Varying one
axis at a time never visits that corner.

### Where the crossover is

The vocabulary ladder, short names, everything else held at the baseline:

```
configuration               ratio   C:A time
v8-short-many-real          600.0     1.57x
v80-short-many-real          73.2     1.14x
v150-short-many-real         39.5     1.01x   <- the crossing
v300-short-many-real         19.9     0.90x
v500-short-many-real         12.0     0.79x
v800-short-many-real          7.5     0.70x
vunique-short-many-real       3.0     0.53x
```

Monotone in the repetition ratio, crossing 1.0 at **R ≈ 40**. Measured, by
interpolation between adjacent rungs, with no model involved.

**The fine rungs are load-bearing.** With only the 8/80/800/unique grid the
interpolation spans a 10x gap and the headline number swung between 38 and 75
across runs on ordinary timing noise. With rungs at R of 40, 20 and 12 it sits
at 34-38 for short names and 44-55 for long ones across repeated runs. Quoting a
single figure to two significant digits would be false precision either way,
which is why the JSON carries the bracketing configurations and their margins
rather than only the interpolated value.

**The ratio floor is 3, not 1**, and that is a property of documents rather than
a gap in the sweep. Even with every class value distinct, the tag name and the
attribute name are reused on every element: `n` elements make `3n` name uses of
`n + 2` distinct names. Reaching R = 1 would need a distinct tag and a distinct
attribute name per element, which is not a page anyone writes.

### The exchange rate, and why the two estimates disagree

The count-based trade needs no machine: **interning costs one crossing per
distinct name and saves `(uses - distinct) x mean-name-length` bytes.** Turning
that into a decision needs `k`, the cost of a crossing in bytes-of-copying, and
then `R* = 1 + k/L`.

`k` is measured from two controlled contrasts inside arm C, each differing in
exactly one axis:

- **crossings**, from the chattiness pair: the same bytes in 1,875 fewer calls
  gives **115-155 ns per crossing**, consistent across two independent pairs.
- **bytes**, from the name-length pair: the same call count carrying up to 136 KB
  more gives **0.4-1.4 ns per byte**.

So `k` lands somewhere around **100-300 bytes per crossing**, and the modelled
`R*` is of order 30-90 for short names and 2-6 for long ones.

**The per-byte figure is the weak one and the range says so.** Copying is a
`memcpy`, these are kilobytes, and the signal sits close to the noise floor, so
`k` moves by a factor of three between runs while the per-crossing figure barely
moves at all. That is itself the most useful thing the contrasts establish:
**bytes are nearly free and crossings are not.** The report prints both marginal
costs and the pair count behind each, so a reader can see which half is solid.

For long names the modelled `R*` of about 5 and the observed crossover of about
50 disagree by an order of magnitude, **and the gap is the finding**. The model
compares interning against not interning inside one ABI; the observation compares
two whole paths. If the atom tier already pays from R = 5 but the native path
does not overtake JavaScript until R = 50, then interning is not what loses in
between. The rest of the path is, and on this harness that means wasmi
interpreting the guest. A compiled guest would move the observed crossover down
toward the modelled one without changing the ABI at all.

A least-squares fit of `time ~ crossings + bytes` over the whole sweep was tried
first and is not what is reported. It returned R² of 0.6 and a per-crossing cost
of two microseconds, an order of magnitude more than a wasmi host call can take:
with configurations differing in shape, step count and style work at once, it
attributed everything it could not explain to whichever regressor moved. Two
points differing in exactly one thing measure that thing.

### What each axis found

- **Vocabulary** is the axis that matters. Everything above is it.
- **Name length** costs the native binding almost nothing until the vocabulary
  is large: at 8 distinct names, going from 3-byte to 70-byte names moves arm C
  by 552 bytes total. Arm A pays 138,000 more.
- **Chattiness** is the cleanest per-call measurement in the sweep, and it is
  what `k` is built from. Delivering the same text in a sixteenth as many calls
  saves both arms real time, which is what a per-call boundary cost looks like.
- **Tree shape** did nothing measurable. Deep chain, wide fan and realistic all
  land within noise of each other at equal node count, on both arms. Worth
  reporting precisely because it is a negative result: the boundary does not
  care what shape the tree is.
- **Event rate** is the native binding's best case. 2,000 dispatches took it
  from 1.03x with listeners attached and idle to **2.05x** under the storm, with
  the byte count unchanged at 11,133, because a click really does copy zero
  bytes. The deferred-dispatch queue did not become the cost. Both points are in
  the sweep so registration is separable from delivery.

## The two upstream operations

Kept as the cited baseline rather than replaced by the sweep: they carry the
provenance the repository rests on, and the paper's original table has to stay
reproducible. Upstream has ten operations; nine measure a reconciler, which is
not what this is about. The two kept are the best and worst case for an
interned-string ABI, so reporting both makes the interning claim falsifiable
rather than flattering.

| | table before | touched | distinct vocabulary strings |
| --- | --- | --- | --- |
| `create-1000` | empty | 1,000 rows | **11** |
| `partial-update` | 10,000 rows | 1,000 labels | **0** |

`create-1000` is dominated by static tag and attribute names: eleven distinct
strings serve twenty-six thousand operations. It is where an interned tier wins
by the largest margin it will ever win by. `partial-update` is dominated by
dynamic text that no design can intern. It is where an interned tier should show
no advantage at all, and it does not.

## The arms

| | what it is | how it is counted |
| --- | --- | --- |
| **A** | JavaScript over the existing script runtime (Boa), against the engine's DOM | measured, in process |
| **B** | Rust/wasm through generated wasm-bindgen glue | **statically derived — never run, never timed** |
| **C** | Rust/wasm through the native host binding (`blitz-wasm`), against the same DOM | measured, in process |

Arms A and C execute in-process against the same document, and the report
asserts they built identical trees before it will print anything.

### Arm B is statically derived

**No arm-B number was observed.** Each is worked out from the call sequence in
`src/calls.rs` and wasm-bindgen's code-generation scheme, is stamped
`"derivation": "static_analysis"` in the JSON, is marked `[derived]` in the
summary, and carries no timing. A test asserts that a derived arm never acquires
one.

The model was read out of the vendored crate sources, not recalled:

- `web-sys-0.3.104/src/features/gen_{Document,Node,Element}.rs` for the
  signatures, which decide how many strings each call carries and whether it
  returns a JS object.
- `wasm-bindgen-0.2.106/src/{lib.rs,convert/slices.rs,cache/intern.rs}` for the
  string and object ABI.

**It is reported in two configurations.** wasm-bindgen has a string cache, but
it is opt-in twice over: `enable-interning` is a non-default cargo feature, and
each string must be passed to `wasm_bindgen::intern` explicitly. Modelling only
the default would be comparing arm C against a wasm-bindgen with its interning
switched off, which is a strawman.

It has not been verified against emitted glue. Doing that needs
`wasm-bindgen-cli`, which this harness deliberately does not install, and the
caveat travels inside the JSON on every arm-B entry rather than only here.

## What is measured

**Counts are the result.** They are properties of the interface and are
identical on every machine that runs this workload.

- DOM operations issued
- boundary crossings
- bytes copied across the boundary
- host allocations

**Timings are secondary** and are reported that way: arms A and C only,
interleaved rather than run in blocks, 24 iterations after 3 warmups, median and
interquartile range and never a bare mean, with drift across the run reported if
the first quarter and last quarter differ by more than 10%. A timing is valid
only against another timing from the same run on the same machine. The chip, OS,
`rustc`, profile, and the `ps-blitz` commit are recorded into every report so
that constraint is checkable rather than remembered.

### Two allocation figures, deliberately

`blitz-wasm`'s ABI.md records that its own allocation counter once read zero
while two `String`s were allocated per attribute write. The lesson taken from
that here is not "count more carefully" but that an instrument recording what
its author chose to record will agree with its author.

So `process_allocations` comes from a global allocator below every crate
involved, where nothing can opt out of being counted, and
`binding_reported_allocations` holds the binding's own figure beside it. On
`create-1000` arm C reports 2,011 and the allocator sees 43,165. **The gap is
the finding**, not a discrepancy: it is everything `blitz-wasm` cannot see from
where it sits, including wasmi's own work and the `String`s the DOM facade
allocates internally. Neither number is the honest one alone.

## Results

From one run on an Apple M4 Max, `release`. **Counts reproduce anywhere;
timings do not.**

### `create-1000` — 11 distinct strings serve 26,000 operations

| arm | DOM ops | crossings | per op | bytes | steady state |
| --- | ---: | ---: | ---: | ---: | ---: |
| A, Boa | 26,000 | 26,000 | 1.00 | 134,834 | 134,834 |
| B, wasm-bindgen default *(derived)* | 26,000 | 52,000 | 2.00 | 134,834 | 134,834 |
| B, wasm-bindgen interned *(derived)* | 26,000 | 52,000 | 2.00 | 20,913 | 20,834 |
| C, native binding | 26,000 | **26,011** | **1.00** | **20,913** | **20,834** |

The eleven crossings above 26,000 in arm C are `intern`, once per distinct name
for the life of the instance, carrying 79 bytes between them. The 114,000-byte
difference is the page's vocabulary, sent once instead of twenty-six thousand
times.

Arm B's crossings are double because a crossing is *one call that leaves the
caller's representation domain*, and the glue path makes two per operation:
wasm to the JavaScript shim, then the shim to the DOM. The second is the same
call arm A makes. The first is what the glue adds, and it is not an
inefficiency in the glue — it is the glue.

That convention is stated because getting it wrong is easy and was. An earlier
model reported 46,000 here: one crossing per call, plus one per returned object
for the `__wbindgen_object_drop_ref` its drop was assumed to call. Checked
against glue emitted by `wasm-bindgen-cli` 0.2.127, that intrinsic does not
exist — objects cross as externrefs, and the externref table is grown once at
init and touched afterwards only inside `handleError`. Dropping the reclamation
charge removes 20,000; counting the shim's own DOM call adds 26,000. Two errors
in opposite directions is why the old figure looked plausible.

### `partial-update` — nothing to intern

| arm | DOM ops | crossings | bytes | steady state |
| --- | ---: | ---: | ---: | ---: |
| A, Boa | 1,000 | 1,000 | 22,104 | 22,104 |
| B, both configurations *(derived)* | 1,000 | 2,000 | 22,104 | 22,104 |
| C, native binding | 1,000 | 1,000 | 22,104 | 22,104 |

**The bytes are identical in every arm.** Text is content, not a name from a
small vocabulary, and no binding design makes it cheaper. The crossings still
separate, because the shim is in the path whatever it carries, but on the axis
this benchmark exists to measure there is nothing here to win. That is why the
pair is the honest unit: `create-1000` alone would be a true table telling a
misleading story.

### Cross-validation

The derived arm lands on the measured arms exactly, in the configurations where
their designs match, and `tests/arms_agree.rs` asserts both:

- wasm-bindgen with interning **off** copies every string on every call, as Boa
  does → **134,834 = 134,834**, arm A.
- wasm-bindgen with interning **on** sends each name once, as the native binding
  does → **20,913 = 20,913**, arm C.

Two exact agreements between a model and two instruments that share no code.
Separately, arm A's counts come from Boa's binding instrumentation and arm C's
from `blitz-wasm`'s counters, and both are checked against `src/calls.rs`, which
neither knows about. Three independent descriptions of one workload agreeing is
worth more than any one of them being carefully written.

## Layout

```
workload/         word lists, seeded generator, row model, and the parameterised
                  scenario generator. No dependencies: linked into both the host
                  harness and the wasm guest, so the arms cannot disagree about
                  the workload.
src/sweep.rs      which configurations run, and why each one is in the list.
src/crossover.rs  the primary analysis: R, the exchange rate, the worst case.
guest/            arm C's guest. A separate workspace: it only builds for
                  wasm32-unknown-unknown, and it must not share the harness's
                  target directory, which the harness holds a lock on while
                  shelling out to build it.
src/arm_a.js      arm A's workload, transcribed from the Rust and checked by
                  the identical-tree test.
src/calls.rs      the operation as arm-independent data. Arm B's input, and
                  the cross-check for arms A and C.
src/arm_b.rs      the static model. Nothing in it runs.
src/alloc.rs      the counting global allocator.
```

## Requirements

- The `wasm32-unknown-unknown` target. `rustup target add
  wasm32-unknown-unknown`; the harness will not install it.
- Network access for the initial build. Cargo fetches the exact `ps-blitz` and
  `resvg` commits recorded in the manifests and lockfiles; no sibling checkout
  or GitHub credentials are required.

Arm A's byte counter is a four-line addition to `blitz-script`
(`script_stats::BoundaryCounters`, incremented at `dom::to_rust_string`, the
single point where a string leaves the JavaScript heap for the host). It is
gated on the same `deep_profiling_enabled` switch as the existing timing
collectors, so a build that is not profiling pays one relaxed atomic load.
Without it arm A has no measured byte figure at all, and deriving one from
reading `arm_a.js` would hold arm A to a weaker standard than arm B.

## Axes that cannot be run

Two axes are named, understood, and **not measured**. Both are places the
handle-and-atom design is most likely to lose, which is exactly why they are
written down. An unmeasured axis that is documented is a finding; one that is
silently absent is a hole a reviewer will find for you.

They are in `results.json` under `unmeasured_axes` as well as here, so a reader
of the data cannot miss them either.

### 1. Read-heavy workloads

**The most likely weakness in the whole design.**

Every write in the ABI moves handles and atoms and copies nothing. A read cannot:
it has to get data back into guest memory, and `blitz-dom-api`'s readers return
owned `String`s by design. A read is therefore expected to cost **two copies**,
one into the host's `String` and one into the guest's linear memory, against
wasm-bindgen's one. A page that reads as much as it writes could erase the entire
interning advantage, and nothing in this harness would show it.

*What blocks it:* the ABI has no read operations at all. There is nothing to
call, so there is nothing to count, and a model of a read would be a guess about
an implementation nobody has written.

*Operations it would need:*

- `get_attribute(node, name_atom) -> atom or string`
- `text_content(node) -> string into guest memory`
- `child_count(node)` / `child_at(node, index) -> handle`
- `get_bounding_client_rect(node) -> geometry` — which also needs a layout flush,
  and the facade does not flush

*Expected finding:* reads are the axis where the native binding loses outright.
An atom-returning `get_attribute` would avoid it for the interned tier; text and
geometry could not.

### 2. Structural churn

Handles are never reused and never freed, by design: a detached node stays
addressable, so a handle to it stays meaningful. That is the right trade for a
page that grows and the wrong one for a list that churns. A
create/destroy/reorder cycle would grow the handle table without bound.

*What blocks it:* there is no `remove` and no `insert_before`, so the cycle
cannot be expressed. `append_child` detaches from the previous parent, which
permits a move but not a removal and not an ordered insertion.

*Operations it would need:*

- `remove_child(parent, child)`
- `insert_before(parent, child, reference)`
- a generational handle, so a freed slot can be reused without a stale handle
  silently addressing a new node

*Expected finding:* handle-table growth proportional to total nodes ever created
rather than nodes alive, and a swap-rows workload that costs a long-lived page
more the longer it runs. wasm-bindgen has no equivalent leak: an externref is
reclaimed by the JavaScript collector once the guest stops holding it, with no
table entry to reuse and nothing for the guest to get wrong.

## What this is not

No Playwright, no Node harness, no web server, no results website, no framework
comparison table, no CI matrix. A binary that runs the arms and writes JSON.

Arm B models the glue, not the DOM implementation underneath it, so it says what
a binding costs and says nothing about what Blink or Gecko then do. No browser
was stood up.
