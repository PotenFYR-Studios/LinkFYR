import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import type { DocBlock } from "../docs/content";
import Code from "./Code";

/** Inline formatting for the markdown-ish block strings: **bold**, `code`, [links](url). */
function renderInline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
  let last = 0;
  let k = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const tok = m[0];
    k++;
    if (tok.startsWith("**")) {
      out.push(<strong key={k}>{tok.slice(2, -2)}</strong>);
    } else if (tok.startsWith("`")) {
      out.push(<code key={k}>{tok.slice(1, -1)}</code>);
    } else {
      const mm = tok.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      if (mm) {
        const href = mm[2];
        out.push(
          href.startsWith("/") ? (
            <Link key={k} to={href}>
              {mm[1]}
            </Link>
          ) : (
            <a key={k} href={href}>
              {mm[1]}
            </a>
          ),
        );
      } else {
        out.push(tok);
      }
    }
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** Shared renderer for the typed DocBlock list (discord-botlists docs architecture). */
export function Block({ block }: { block: DocBlock }) {
  switch (block.type) {
    case "text":
      return <p className="md-text">{renderInline(block.content)}</p>;
    case "h3":
      return <h3 className="md-h3">{renderInline(block.content)}</h3>;
    case "code":
      return <Code content={block.content} lang={block.lang} />;
    case "table":
      return (
        <div className="md-table-wrap">
          <table className="md-table">
            <thead>
              <tr>
                {block.headers.map((h, i) => (
                  <th key={i}>{renderInline(h)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, i) => (
                <tr key={i}>
                  {row.map((cell, j) => (
                    <td key={j}>{renderInline(cell)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "list":
      return (
        <ul className="md-list">
          {block.items.map((item, i) => (
            <li key={i}>{renderInline(item)}</li>
          ))}
        </ul>
      );
    case "note":
      return (
        <div className={`md-note md-note-${block.tone}`}>
          <span className="md-note-label">
            {block.tone === "warn" ? "Note" : block.tone === "tip" ? "Tip" : "Info"}
          </span>
          <span>{renderInline(block.content)}</span>
        </div>
      );
  }
}
