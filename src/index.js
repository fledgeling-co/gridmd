import { parseDocument } from './parser.js';
import { validateDocument } from './validate.js';

export { parseDocument } from './parser.js';
export { validateDocument, isValidPartPath } from './validate.js';
export { parseScalar, splitCached, ERROR_VALUES } from './scalar.js';
export { parseTarget, parseCell, colToNum, numToCol, MAX_COL, MAX_ROW } from './refs.js';
export { findPropsSplit, tryProps, splitPipeRow, parseInfoArgs, parseYaml, RESERVED_KINDS } from './parser.js';

export function lint(source, opts = {}) {
  const doc = parseDocument(source, opts);
  validateDocument(doc);
  const byLine = (a, b) => a.line - b.line;
  return {
    doc,
    errors: [...doc.errors].sort(byLine),
    warnings: [...doc.warnings].sort(byLine),
    sheets: doc.sheets.length,
    cells: doc.stats?.defs ?? 0,
    blocks: doc.stats?.blocks ?? 0,
  };
}
