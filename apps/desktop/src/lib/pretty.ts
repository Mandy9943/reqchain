// Response-body pretty-printing (design spec §8: "pretty-print and search for
// JSON, XML, HTML and plain text"). JSON is handled by `JSON.stringify`; plain
// text needs nothing; this module covers the markup cases.
//
// Deliberately dependency-free. A full HTML parser would be a large dependency
// for a read-only indenter, and it would also "fix" malformed markup — exactly
// the thing someone debugging an API response needs to SEE rather than have
// silently repaired. This only re-indents: every byte of the original document
// survives, only the whitespace between tokens changes.

/** Which pretty-printer, if any, applies to a response body. */
export type BodyKind = "json" | "markup" | "text";

const MARKUP_TYPES = [
  "xml",
  "html",
  "xhtml",
  "svg",
  "atom",
  "rss",
  "soap",
  "wsdl",
];

/**
 * Picks a printer from the Content-Type first and the body's own shape second.
 * Content-Type wins when it says something useful, because a server that
 * declares `application/xml` means it even if the payload starts with a
 * comment; sniffing is the fallback for the many APIs that send
 * `text/plain` or nothing at all.
 */
export function detectBodyKind(
  contentType: string | undefined,
  body: string,
): BodyKind {
  const type = (contentType ?? "").toLowerCase();
  if (type.includes("json")) return "json";
  if (MARKUP_TYPES.some((t) => type.includes(t))) return "markup";

  const head = body.trimStart();
  if (head.startsWith("{") || head.startsWith("[")) {
    // Only claim JSON if it actually parses — a body that merely starts with a
    // brace would otherwise render as "not pretty-printable" with no
    // explanation.
    try {
      JSON.parse(body);
      return "json";
    } catch {
      /* fall through */
    }
  }
  if (head.startsWith("<")) return "markup";
  return "text";
}

type Token =
  | { kind: "open"; text: string; name: string; selfClosing: boolean }
  | { kind: "close"; text: string; name: string }
  | { kind: "standalone"; text: string } // comment, doctype, PI, CDATA
  | { kind: "text"; text: string };

/**
 * HTML elements that never have a closing tag. In XML every element closes, so
 * this list only ever *removes* indentation that would otherwise be wrong for
 * HTML — an XML document containing an element called `link` would be indented
 * as if it were empty, which is a cosmetic mismatch, not lost content.
 */
const VOID_ELEMENTS = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

/** Elements whose contents are significant whitespace and must be left alone. */
const PRESERVED_ELEMENTS = new Set(["pre", "textarea", "script", "style"]);

/** Finds the end of the construct starting at `<`, respecting quotes. */
function endOfTag(source: string, start: number): number {
  for (const [open, close] of [
    ["<!--", "-->"],
    ["<![CDATA[", "]]>"],
  ] as const) {
    if (source.startsWith(open, start)) {
      const end = source.indexOf(close, start + open.length);
      return end === -1 ? source.length : end + close.length;
    }
  }
  let quote: string | null = null;
  for (let i = start + 1; i < source.length; i++) {
    const ch = source[i];
    if (quote) {
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === ">") {
      return i + 1;
    }
  }
  return source.length; // unterminated tag: keep the rest verbatim
}

function tagName(tag: string): string {
  const match = /^<\/?\s*([^\s/>]+)/.exec(tag);
  return match ? match[1].toLowerCase() : "";
}

function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < source.length) {
    if (source[i] === "<") {
      const end = endOfTag(source, i);
      const text = source.slice(i, end);
      i = end;
      if (/^<[!?]/.test(text)) {
        tokens.push({ kind: "standalone", text });
      } else if (text.startsWith("</")) {
        tokens.push({ kind: "close", text, name: tagName(text) });
      } else {
        tokens.push({
          kind: "open",
          text,
          name: tagName(text),
          selfClosing: /\/\s*>$/.test(text),
        });
      }
      continue;
    }
    const next = source.indexOf("<", i);
    const end = next === -1 ? source.length : next;
    tokens.push({ kind: "text", text: source.slice(i, end) });
    i = end;
  }
  return tokens;
}

/**
 * Re-indents an XML or HTML document. Never reorders, drops or rewrites
 * anything: only the whitespace between tokens changes, and the content of
 * `<pre>`, `<textarea>`, `<script>` and `<style>` is copied through untouched.
 *
 * Malformed input is indented as best it can be rather than rejected — a
 * mismatched closing tag simply stops the indentation going negative.
 */
export function indentMarkup(source: string, indent = "  "): string {
  const tokens = tokenize(source);
  const lines: string[] = [];
  let depth = 0;
  const pad = () => indent.repeat(Math.max(0, depth));

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];

    if (token.kind === "text") {
      const trimmed = token.text.trim();
      if (trimmed) lines.push(pad() + trimmed);
      continue;
    }

    if (token.kind === "standalone") {
      lines.push(pad() + token.text.trim());
      continue;
    }

    if (token.kind === "close") {
      depth--;
      lines.push(pad() + token.text.trim());
      continue;
    }

    // An open tag. Whitespace-significant elements are copied verbatim.
    if (PRESERVED_ELEMENTS.has(token.name)) {
      const closeIndex = tokens.findIndex(
        (t, j) => j > i && t.kind === "close" && t.name === token.name,
      );
      if (closeIndex !== -1) {
        const verbatim = tokens
          .slice(i, closeIndex + 1)
          .map((t) => t.text)
          .join("");
        lines.push(pad() + verbatim);
        i = closeIndex;
        continue;
      }
    }

    lines.push(pad() + token.text.trim());
    if (token.selfClosing || VOID_ELEMENTS.has(token.name)) continue;

    // `<title>Hello</title>` stays on one line: an element holding nothing but
    // a short run of text reads far better unsplit, and splitting it is the
    // main thing that makes naive indenters unpleasant to read.
    const next = tokens[i + 1];
    const after = tokens[i + 2];
    if (
      next?.kind === "text" &&
      after?.kind === "close" &&
      after.name === token.name &&
      !next.text.includes("\n") &&
      next.text.trim().length <= 80
    ) {
      lines[lines.length - 1] += next.text.trim() + after.text.trim();
      i += 2;
      continue;
    }

    depth++;
  }

  return lines.join("\n");
}

/**
 * Pretty-prints `body` according to `kind`, returning the body unchanged when
 * there is nothing to do or when the input does not parse. Never throws: a
 * response viewer must show whatever arrived, however malformed.
 */
export function prettyPrint(body: string, kind: BodyKind): string {
  try {
    if (kind === "json") return JSON.stringify(JSON.parse(body), null, 2);
    if (kind === "markup") return indentMarkup(body);
  } catch {
    /* fall through to the raw body */
  }
  return body;
}
