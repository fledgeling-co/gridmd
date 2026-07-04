// Minimal non-validating XML parser for OOXML parts. Namespace prefixes are
// stripped from element names; attributes keep their original keys with a
// local-name lookup helper. Good for well-formed machine-written XML only.

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };

export function decodeEntities(s) {
  return s.replace(/&(#x?[0-9a-fA-F]+|[a-z]+);/g, (m, body) => {
    if (body[0] === '#') {
      const code = body[1] === 'x' || body[1] === 'X'
        ? parseInt(body.slice(2), 16) : parseInt(body.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : m;
    }
    return ENTITIES[body] ?? m;
  });
}

const TOKEN_RE = /<!\[CDATA\[([\s\S]*?)\]\]>|<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<!DOCTYPE[^>]*>|<\/([^>\s]+)\s*>|<([^>\s/]+)([^>]*?)(\/?)>|([^<]+)/g;
const ATTR_RE = /([^\s=]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

export function parseXml(src) {
  const root = { name: '#root', attrs: {}, children: [], text: '' };
  const stack = [root];
  let m;
  TOKEN_RE.lastIndex = 0;
  while ((m = TOKEN_RE.exec(src))) {
    const top = stack[stack.length - 1];
    if (m[1] !== undefined) { top.text += m[1]; continue; }           // CDATA
    if (m[2] !== undefined) {                                          // close
      if (stack.length > 1) stack.pop();
      continue;
    }
    if (m[3] !== undefined) {                                          // open
      const el = { name: local(m[3]), attrs: {}, children: [], text: '' };
      let am;
      ATTR_RE.lastIndex = 0;
      while ((am = ATTR_RE.exec(m[4] ?? ''))) {
        el.attrs[am[1]] = decodeEntities(am[2] ?? am[3] ?? '');
      }
      top.children.push(el);
      if (!m[5]) stack.push(el);
      continue;
    }
    if (m[6] !== undefined) top.text += decodeEntities(m[6]);          // text
  }
  return root.children[0] ?? root;
}

const local = (name) => name.includes(':') ? name.slice(name.indexOf(':') + 1) : name;

// Attribute by local name ('id' matches both id and r:id — exact key wins).
export function attr(el, name) {
  if (el.attrs[name] !== undefined) return el.attrs[name];
  for (const [k, v] of Object.entries(el.attrs)) {
    if (local(k) === name) return v;
  }
  return undefined;
}

export const one = (el, name) => el.children.find((c) => c.name === name) ?? null;
export const all = (el, name) => el.children.filter((c) => c.name === name);

// Deep text of an element (its text + descendants').
export function textOf(el) {
  if (!el) return '';
  let out = el.text;
  for (const c of el.children) out += textOf(c);
  return out;
}

// First descendant (or self) with the given local name.
export function findDeep(el, name) {
  if (el.name === name) return el;
  for (const c of el.children) {
    const found = findDeep(c, name);
    if (found) return found;
  }
  return null;
}
