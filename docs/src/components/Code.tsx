/** Code block renderer with a subtle title bar, org docs style. */
export default function Code({
  content,
  lang,
  title,
}: {
  content: string;
  lang: string;
  title?: string;
}) {
  return (
    <div className="code-block">
      <div className="code-head">
        <span className="code-lang">{title ?? lang}</span>
      </div>
      <pre>
        <code>{content}</code>
      </pre>
    </div>
  );
}
