// Flowmark viewer. Renders a view model (crates/flowmark_parser/src/viewmodel.rs) into a DOM element.
//
//   Flowmark.mount(element, model, { level: 1 })
//
// No dependencies, no build step. The model already contains everything: tokenized rows, blocks with
// levels/badges/risks, and in diff mode added/removed rows and a summary of what changed.
(function (global) {
  "use strict";

  const LEVELS = [
    { n: 0, name: "Card", hint: "Do I need to look, and where is the danger?" },
    { n: 1, name: "Flow", hint: "What happens, in which order, and what can go wrong?" },
    { n: 2, name: "Detail", hint: "Is the L1 story actually true?" },
    { n: 3, name: "Full", hint: "What exactly does every step do?" },
  ];

  function h(tag, attrs, ...children) {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v == null || v === false) continue;
      if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
      else if (k === "class") el.className = v;
      else el.setAttribute(k, v === true ? "" : v);
    }
    for (const c of children.flat(Infinity)) {
      if (c == null || c === false) continue;
      el.append(c instanceof Node ? c : document.createTextNode(String(c)));
    }
    return el;
  }

  const sigilClass = (s) => (s === "?" ? "unsure" : s ? "risk" : "");

  class Viewer {
    constructor(root, model, opts) {
      this.root = root;
      this.m = model;
      this.diff = !!model.changes;
      const fromHash = /^#L([0-3])$/.exec(global.location ? global.location.hash : "");
      this.level = fromHash ? Number(fromHash[1]) : opts.level ?? 1;
      this.expanded = new Set();
      this.inline = new Set();
      this.showChanges = this.diff;
      this.children = model.blocks.map(() => []);
      for (const b of model.blocks) if (b.parent != null) this.children[b.parent].push(b.id);
      this.startsAt = new Map();
      for (const b of model.blocks) {
        if (!this.startsAt.has(b.head_row)) this.startsAt.set(b.head_row, []);
        this.startsAt.get(b.head_row).push(b.id);
      }
      // Changed rows per block (for Δ badges and "show changes").
      this.changed = model.blocks.map((b) => {
        let n = 0;
        for (let i = b.first_row; i <= b.last_row; i++) if (model.rows[i].change && model.rows[i].kind !== "blank") n++;
        return n;
      });
      this.onKey = (e) => this.key(e);
      document.addEventListener("keydown", this.onKey);
    }

    destroy() {
      document.removeEventListener("keydown", this.onKey);
      this.root.replaceChildren();
    }

    key(e) {
      if (e.target && /INPUT|TEXTAREA/.test(e.target.tagName)) return;
      if (/^[0-3]$/.test(e.key)) this.setLevel(Number(e.key));
      else if (e.key === "c" && this.diff) {
        this.showChanges = !this.showChanges;
        this.render();
      } else if (e.key === "Escape") {
        this.expanded.clear();
        this.inline.clear();
        this.render();
      }
    }

    setLevel(n) {
      this.level = n;
      this.expanded.clear();
      this.inline.clear();
      if (global.history && global.location) global.history.replaceState(null, "", "#L" + n);
      this.render();
    }

    // ─── visibility ────────────────────────────────────────────────────────

    /// For every block: the outermost hidden ancestor (or -1) and the level its content is shown at.
    state(extra) {
      const { blocks } = this.m;
      const hiddenBy = new Array(blocks.length);
      const vl = new Array(blocks.length);
      for (const b of blocks) {
        const vp = b.parent == null ? this.level : vl[b.parent];
        const ph = b.parent == null ? -1 : hiddenBy[b.parent];
        if (ph !== -1) {
          hiddenBy[b.id] = ph;
          vl[b.id] = vp;
          continue;
        }
        const exp =
          this.expanded.has(b.id) || (extra && extra.has(b.id)) || (this.showChanges && this.changed[b.id] > 0);
        const shown = exp || b.level <= vp;
        hiddenBy[b.id] = shown ? -1 : b.id;
        vl[b.id] = shown && exp ? Math.max(vp, b.level) : vp;
      }
      return { hiddenBy, vl };
    }

    rowVisible(i, st) {
      const r = this.m.rows[i];
      if (r.block != null && st.hiddenBy[r.block] !== -1) return false;
      if (this.showChanges && r.change) return true;
      const vl = r.block == null ? this.level : st.vl[r.block];
      return r.level == null || r.level <= vl;
    }

    commentVisible(c, row, st) {
      if (c.sigil === "!" || c.sigil === "!?") return true; // risks show whenever their line shows
      const vl = row.block == null ? this.level : st.vl[row.block];
      return c.level == null || c.level <= vl;
    }

    /// A visible block whose inside is completely hidden renders as one line.
    collapsible(id, st) {
      const b = this.m.blocks[id];
      if (st.hiddenBy[id] !== -1 || b.last_row <= b.head_row + 1) return false;
      const kids = this.children[id];
      if (!kids.length || !kids.every((k) => st.hiddenBy[k] !== -1)) return false;
      for (let i = b.head_row + 1; i < b.last_row; i++) {
        const r = this.m.rows[i];
        if (r.kind !== "blank" && this.rowVisible(i, st)) return false;
      }
      return true;
    }

    // ─── render ────────────────────────────────────────────────────────────

    render() {
      const scroll = global.scrollY || 0;
      this.root.replaceChildren(this.header(), this.level === 0 ? this.cardView() : this.codeView());
      if (global.scrollTo) global.scrollTo(0, scroll);
    }

    header() {
      const m = this.m;
      const risks = m.risks.filter((r) => r.change !== "removed").length;
      const errors = m.diagnostics.filter((d) => d.severity === "error").length;
      const warnings = m.diagnostics.length - errors;
      return h(
        "header",
        { class: "fm-header" },
        h(
          "div",
          { class: "fm-title" },
          h("strong", {}, m.title || m.file),
          h("span", { class: "fm-file" }, m.base_file ? `${m.base_file} → ${m.file}` : m.file),
        ),
        h(
          "nav",
          { class: "fm-levels", "aria-label": "Detail level" },
          LEVELS.map((l) =>
            h(
              "button",
              {
                class: l.n === this.level ? "active" : "",
                title: `${l.hint} (key ${l.n})`,
                onclick: () => this.setLevel(l.n),
              },
              `L${l.n} ${l.name}`,
            ),
          ),
        ),
        h(
          "div",
          { class: "fm-counts" },
          this.diff &&
            h(
              "label",
              { class: "fm-toggle", title: "Show changed lines at every level (key c)" },
              h("input", {
                type: "checkbox",
                checked: this.showChanges,
                onchange: (e) => {
                  this.showChanges = e.target.checked;
                  this.render();
                },
              }),
              " show changes",
            ),
          h("button", { class: "fm-count risk", title: "Risks", onclick: () => this.setLevel(0) }, `${risks} !`),
          errors || warnings
            ? h(
              "button",
              { class: "fm-count " + (errors ? "error" : "warn"), title: "Checker findings", onclick: () => this.setLevel(0) },
              errors ? `${errors} ✗` : "",
              warnings ? ` ${warnings} ⚠` : "",
            )
            : null,
        ),
      );
    }

    // ─── L0 ────────────────────────────────────────────────────────────────

    cardView() {
      const m = this.m;
      const parts = [];
      if (this.diff) parts.push(this.changesView());
      const card = m.card;
      if (card) {
        const changed = new Map((m.changes ? m.changes.card : []).map((c) => [c.name, c]));
        parts.push(
          h(
            "section",
            { class: "fm-card" },
            h("h1", {}, card.title),
            card.summary.map((s) => h("p", {}, s)),
            h(
              "table",
              {},
              card.fields.map((f) =>
                h(
                  "tr",
                  { class: changed.has(f.name) ? "changed" : "" },
                  h("th", {}, f.name),
                  h(
                    "td",
                    {},
                    changed.has(f.name) && changed.get(f.name).base != null
                      ? h("del", {}, changed.get(f.name).base)
                      : null,
                    this.cardItems(f),
                  ),
                ),
              ),
            ),
          ),
        );
      } else {
        parts.push(h("p", { class: "fm-empty" }, "This file has no review card."));
      }
      parts.push(
        h(
          "section",
          { class: "fm-list" },
          h("h2", {}, "Risks"),
          m.risks.length
            ? h(
                "ul",
                {},
                m.risks.map((r) =>
                  h(
                    "li",
                    { class: [sigilClass(r.kind), r.change || ""].join(" ") },
                    h(
                      "a",
                      { href: "#", onclick: (e) => (e.preventDefault(), this.reveal(r.row)) },
                      h("span", { class: "sigil" }, r.kind),
                      " ",
                      r.text,
                    ),
                    r.owner && h("span", { class: "owner" }, "→ " + r.owner),
                    r.change && h("span", { class: "change-tag" }, r.change),
                  ),
                ),
              )
            : h("p", { class: "fm-empty" }, "No risks marked."),
        ),
      );
      if (m.diagnostics.length) {
        parts.push(
          h(
            "section",
            { class: "fm-list" },
            h("h2", {}, "Checker"),
            h(
              "ul",
              {},
              m.diagnostics.map((d) =>
                h(
                  "li",
                  { class: d.severity },
                  h(
                    "a",
                    { href: "#", onclick: (e) => (e.preventDefault(), d.row != null && this.reveal(d.row)) },
                    h("span", { class: "sigil" }, d.severity === "error" ? "✗" : "⚠"),
                    ` ${d.code} ${d.message}`,
                  ),
                  h("span", { class: "owner" }, "line " + d.line),
                ),
              ),
            ),
          ),
        );
      }
      return h("main", { class: "fm-l0" }, parts);
    }

    cardItems(f) {
      if (!f.items.length) return h("span", { class: "muted" }, f.raw || "none");
      const out = [];
      f.items.forEach((it, i) => {
        if (i) out.push(h("span", { class: "sep" }, ", "));
        out.push(
          h(
            "span",
            { class: "item" + (it.risk ? " risk" : "") },
            it.kind && h("span", { class: "kind" }, it.kind + " "),
            it.name,
            it.note && h("span", { class: "note" }, ` (${it.note})`),
            it.risk && h("span", { class: "sigil" }, " !"),
          ),
        );
      });
      return out;
    }

    changesView() {
      const c = this.m.changes;
      const items = [];
      for (const f of c.card)
        items.push(
          h(
            "li",
            {},
            h("span", { class: "what" }, "card " + f.name),
            f.base != null ? h("del", {}, f.base) : h("span", { class: "muted" }, "(new)"),
            " → ",
            f.head != null ? h("ins", {}, f.head) : h("span", { class: "muted" }, "(removed)"),
          ),
        );
      for (const r of c.risks_added) items.push(h("li", { class: "risk" }, h("span", { class: "what" }, "new risk"), r));
      for (const r of c.risks_removed)
        items.push(h("li", { class: "removed" }, h("span", { class: "what" }, "removed risk"), h("del", {}, r)));
      for (const e of c.entries)
        items.push(h("li", {}, h("span", { class: "what" }, `entry ${e.name} ${e.change}`), e.details.join("; ")));
      for (const t of c.types)
        items.push(h("li", {}, h("span", { class: "what" }, `type ${t.name} ${t.change}`), t.details.join(", ")));
      items.push(
        h("li", { class: "muted" }, h("span", { class: "what" }, "lines"), `+${c.lines_added} −${c.lines_removed}`),
      );
      return h("section", { class: "fm-changes" }, h("h2", {}, "What changed"), h("ul", {}, items));
    }

    // ─── L1–L3 ─────────────────────────────────────────────────────────────

    codeView() {
      const st = this.state();
      const code = h("div", { class: "fm-code" });
      this.renderRange(0, this.m.rows.length - 1, st, code, false);
      // Section headings show when their section has visible content.
      const kids = [...code.children];
      kids.forEach((el, i) => {
        if (!el.classList.contains("section")) return;
        const next = kids.slice(i + 1).find((k) => !k.classList.contains("blank"));
        if (!next || next.classList.contains("section")) el.remove();
      });
      while (code.firstChild && code.firstChild.classList.contains("blank")) code.firstChild.remove();
      const main = h("main", { class: "fm-l" + this.level });
      if (this.diff) main.append(this.changesView());
      main.append(code);
      return main;
    }

    renderRange(from, to, st, out, nested) {
      const { rows, blocks } = this.m;
      let lastBlank = true;
      let group = null; // placeholder group being built
      const flush = () => {
        if (group) out.append(...this.placeholder(group, st));
        group = null;
      };
      for (let i = from; i <= to; i++) {
        const r = rows[i];
        if (r.kind === "blank") {
          if (r.level === 0) continue; // version line and card
          if (r.block != null && st.hiddenBy[r.block] !== -1) continue;
          if (!lastBlank && !group) {
            out.append(h("div", { class: "row blank" }));
            lastBlank = true;
          }
          continue;
        }
        if (r.block != null && st.hiddenBy[r.block] !== -1) {
          const hid = st.hiddenBy[r.block];
          const b = blocks[hid];
          if (i === b.first_row) {
            if (b.helper && !nested) continue; // shown at its call sites
            if (group && group.parent === b.parent) group.ids.push(hid);
            else {
              flush();
              group = { parent: b.parent, ids: [hid], row: i };
            }
          }
          continue;
        }
        if (r.kind !== "section" && !this.rowVisible(i, st)) continue;
        flush();
        // A block starting here whose inside is fully hidden: one line.
        const starting = this.startsAt.get(i) || [];
        const col = starting.find((id) => this.collapsible(id, st));
        if (col != null) {
          out.append(...this.collapsedRow(col, st));
          i = blocks[col].last_row;
          lastBlank = false;
          continue;
        }
        out.append(this.row(i, st));
        lastBlank = false;
        for (const helper of r.calls) {
          if (st.hiddenBy[helper] === -1) continue;
          const key = i + ":" + helper;
          if (this.inline.has(key)) {
            const hb = blocks[helper];
            const inner = h("div", { class: "fm-inline", style: `margin-left:calc(4.7em + ${r.indent + 2}ch)` });
            this.renderRange(hb.first_row, hb.last_row, this.state(new Set([helper])), inner, true);
            out.append(inner);
          } else {
            out.append(...this.bubbledRisks([helper], st, r.indent + 2));
          }
        }
      }
      flush();
    }

    placeholder(group, st) {
      const { blocks, rows } = this.m;
      const bs = group.ids.map((id) => blocks[id]);
      const lines = bs.reduce((n, b) => n + b.lines, 0);
      const label =
        bs.length === 1 && bs[0].parent == null ? `${bs[0].label} (${plural(lines, "line")})` : plural(lines, "line");
      const indent = rows[bs[0].head_row].indent;
      const el = h(
        "div",
        {
          class: "row placeholder",
          title: "Click to expand",
          onclick: () => {
            for (const id of group.ids) this.expanded.add(id);
            this.render();
          },
        },
        h("span", { class: "ln" }),
        h("span", { class: "mark" }),
        h("span", { class: "code" }, " ".repeat(indent), "⋯ ", label, this.badges(bs, st)),
      );
      return [el, ...this.bubbledRisks(group.ids, st, indent + 2)];
    }

    collapsedRow(id, st) {
      const b = this.m.blocks[id];
      const first = this.m.rows[b.head_row];
      const last = this.m.rows[b.last_row];
      const kids = this.children[id].map((k) => this.m.blocks[k]);
      const el = this.row(b.head_row, st, {
        tail: [h("span", { class: "fold", title: "Click to expand" }, ` … ${b.summary} `), ...segs(last.code, true)],
        onclick: () => {
          for (const k of this.children[id]) this.expanded.add(k);
          this.render();
        },
        badges: this.badges(kids, st),
      });
      el.classList.add("collapsed");
      return [el, ...this.bubbledRisks(this.children[id], st, first.indent + 2)];
    }

    badges(bs, st) {
      const parts = [];
      const seen = new Set();
      for (const b of bs)
        for (const x of b.badges)
          if (!seen.has(x)) {
            seen.add(x);
            parts.push(x);
          }
      const risks = bs.reduce((n, b) => n + b.risks.length, 0);
      const unsure = bs.reduce((n, b) => n + b.unsure, 0);
      const delta = bs.reduce((n, b) => n + this.changed[b.id], 0);
      if (!parts.length && !risks && !unsure && !delta) return null;
      return h(
        "span",
        { class: "badges" },
        "▸ ",
        parts.join(" · "),
        risks ? h("span", { class: "b-risk" }, (parts.length ? " · " : "") + risks + " !") : null,
        unsure ? h("span", { class: "b-unsure" }, " · " + unsure + " ?") : null,
        delta ? h("span", { class: "b-delta" }, " · Δ" + delta) : null,
      );
    }

    /// Hidden risks inside `ids`, shown under the placeholder / call (SPEC §9.4).
    bubbledRisks(ids, st, indent) {
      const out = [];
      for (const id of ids)
        for (const rid of this.m.blocks[id].risks) {
          const risk = this.m.risks[rid];
          if (this.rowVisible(risk.row, st)) continue;
          out.push(
            h(
              "div",
              { class: "row bubbled " + sigilClass(risk.kind), onclick: () => this.reveal(risk.row) },
              h("span", { class: "ln" }),
              h("span", { class: "mark" }),
              h("span", { class: "code" }, " ".repeat(indent), h("span", { class: "sigil" }, risk.kind), " ", risk.text),
            ),
          );
        }
      return out;
    }

    row(i, st, extra) {
      const r = this.m.rows[i];
      const cls = ["row", r.kind];
      if (r.change) cls.push(r.change);
      const sig = r.comments.find((c) => c.sigil && this.commentVisible(c, r, st));
      if (sig) cls.push(sigilClass(sig.sigil));
      const lineNo = r.line != null ? r.line : r.base_line;
      const el = h(
        "div",
        { class: cls.join(" "), "data-row": i, onclick: extra && extra.onclick },
        h("span", { class: "ln", title: r.line == null ? "base line" : null }, lineNo),
        h("span", { class: "mark" }, r.change === "added" ? "+" : r.change === "removed" ? "−" : ""),
      );
      const code = h("span", { class: "code" });
      if (r.kind === "section") {
        code.append(...segs(r.code), h("span", { class: "section-name" }, r.comments[0].section));
        el.append(code);
        return el;
      }
      code.append(...segs(r.code));
      if (extra && extra.tail) code.append(...extra.tail);
      const comments = r.comments.filter((c) => this.commentVisible(c, r, st)).map((c) => this.comment(c));
      const shown = comments.filter((c) => c);
      if (shown.length) code.append(h("span", { class: "cmt" }, r.kind === "code" ? "  // " : "// ", shown));
      el.append(code);
      if (extra && extra.badges) code.append(extra.badges);
      // Calls to hidden helpers: badges + a toggle to expand the helper here.
      for (const helper of r.calls || []) {
        if (st.hiddenBy[helper] === -1) continue;
        const key = i + ":" + helper;
        const b = this.badges([this.m.blocks[helper]], st);
        code.append(
          h(
            "span",
            {
              class: "helper-toggle",
              title: "Show this helper here",
              onclick: (e) => {
                e.stopPropagation();
                this.inline.has(key) ? this.inline.delete(key) : this.inline.add(key);
                this.render();
              },
            },
            this.inline.has(key) ? "▾ " : "",
            b || "▸ " + this.m.blocks[helper].label,
          ),
        );
      }
      return el;
    }

    comment(c) {
      if (c.plain) return h("span", { class: "text" }, c.text);
      const parts = [];
      if (c.sigil) parts.push(h("span", { class: "sigil" }, c.sigil), " ");
      if (c.tag) parts.push(h("span", { class: "tag" }, c.tag), " ");
      if (c.text) parts.push(h("span", { class: "text" }, c.text));
      if (c.marker) parts.push(" ", h("span", { class: "marker", title: "detail-level marker" }, "L" + c.marker));
      for (const r of c.refs) parts.push(" ", refChip(r));
      if (!c.text && !c.sigil && !c.tag && !c.marker && !c.refs.length) return null;
      if (!c.text && !c.sigil && !c.tag && !c.marker) return h("span", { class: "refs-only" }, parts);
      return h("span", { class: "comment" }, parts);
    }

    /// Show a row: leave L0, expand every hidden block around it, raise the level if needed.
    reveal(rowIndex) {
      if (this.level === 0) this.level = 1;
      const r = this.m.rows[rowIndex];
      for (let b = r.block; b != null; b = this.m.blocks[b].parent) this.expanded.add(b);
      let st = this.state();
      if (!this.rowVisible(rowIndex, st) && r.level != null) {
        this.level = Math.max(this.level, r.level);
        st = this.state();
      }
      if (global.history && global.location) global.history.replaceState(null, "", "#L" + this.level);
      this.render();
      const el = this.root.querySelector(`[data-row="${rowIndex}"]`);
      if (el) {
        if (el.scrollIntoView) el.scrollIntoView({ block: "center" });
        el.classList.add("flash");
        setTimeout(() => el.classList.remove("flash"), 1500);
      }
    }
  }

  function segs(list, trimStart) {
    return list.map((s, i) => {
      const text = trimStart && i === 0 ? s.text.trimStart() : s.text;
      return s.class ? h("span", { class: s.class }, text) : document.createTextNode(text);
    });
  }

  function refChip(r) {
    const lines = [];
    if (r.target) lines.push("target: " + r.target);
    if (r.quote) lines.push("quote: " + r.quote);
    if (!r.target && r.resolved) lines.push("searched in: " + r.resolved);
    if (r.defines_scope) lines.push("(scope for quote-only refs inside)");
    const text = [r.target, r.quote && `"${r.quote}"`].filter(Boolean).join(" ");
    return h(
      "span",
      {
        class: "ref" + (r.defines_scope ? " scope" : ""),
        title: lines.join("\n") + "\nclick to copy",
        onclick: (e) => {
          e.stopPropagation();
          if (navigator.clipboard) navigator.clipboard.writeText(text);
        },
      },
      "↗ ",
      r.label,
    );
  }

  function plural(n, word) {
    return `${n} ${word}${n === 1 ? "" : "s"}`;
  }

  global.Flowmark = {
    mount(root, model, opts) {
      const v = new Viewer(root, model, opts || {});
      v.render();
      return v;
    },
  };
})(typeof window !== "undefined" ? window : globalThis);
