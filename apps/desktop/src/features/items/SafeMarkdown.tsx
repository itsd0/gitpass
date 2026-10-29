import React, { useMemo } from 'react';
import { marked, type Token, type Tokens } from 'marked';
import { cn } from '../../lib/utils';

export function isSafeUrl(url: string | null | undefined): boolean {
  if (!url) return false;
  const trimmed = url.trim().toLowerCase();
  return (
    trimmed.startsWith('http://') ||
    trimmed.startsWith('https://') ||
    trimmed.startsWith('mailto:')
  );
}

function renderToken(token: Token, key: string): React.ReactNode {
  switch (token.type) {
    case 'heading': {
      const headingToken = token as Tokens.Heading;
      const inner = renderTokens(headingToken.tokens, `${key}-h`);
      switch (headingToken.depth) {
        case 1:
          return (
            <h1 key={key} className="mt-3 mb-1.5 text-xl font-bold text-fg first:mt-0">
              {inner}
            </h1>
          );
        case 2:
          return (
            <h2 key={key} className="mt-2.5 mb-1 text-lg font-semibold text-fg first:mt-0">
              {inner}
            </h2>
          );
        case 3:
          return (
            <h3 key={key} className="mt-2 mb-1 text-base font-semibold text-fg first:mt-0">
              {inner}
            </h3>
          );
        case 4:
          return (
            <h4 key={key} className="mt-1.5 mb-0.5 text-sm font-semibold text-fg first:mt-0">
              {inner}
            </h4>
          );
        case 5:
          return (
            <h5 key={key} className="mt-1 mb-0.5 text-xs font-semibold text-fg first:mt-0">
              {inner}
            </h5>
          );
        case 6:
        default:
          return (
            <h6 key={key} className="mt-1 mb-0.5 text-xs font-semibold uppercase tracking-wider text-muted first:mt-0">
              {inner}
            </h6>
          );
      }
    }

    case 'paragraph': {
      const pToken = token as Tokens.Paragraph;
      return (
        <p key={key} className="my-1.5 leading-relaxed text-[14px] first:mt-0 last:mb-0 break-words">
          {renderTokens(pToken.tokens, `${key}-p`)}
        </p>
      );
    }

    case 'text': {
      const textToken = token as Tokens.Text;
      if (textToken.tokens && textToken.tokens.length > 0) {
        return <React.Fragment key={key}>{renderTokens(textToken.tokens, `${key}-t`)}</React.Fragment>;
      }
      return textToken.text;
    }

    case 'strong': {
      const strongToken = token as Tokens.Strong;
      return (
        <strong key={key} className="font-semibold text-fg">
          {renderTokens(strongToken.tokens, `${key}-s`)}
        </strong>
      );
    }

    case 'em': {
      const emToken = token as Tokens.Em;
      return (
        <em key={key} className="italic">
          {renderTokens(emToken.tokens, `${key}-em`)}
        </em>
      );
    }

    case 'del': {
      const delToken = token as Tokens.Del;
      return (
        <del key={key} className="line-through text-muted">
          {renderTokens(delToken.tokens, `${key}-del`)}
        </del>
      );
    }

    case 'codespan': {
      const codeSpan = token as Tokens.Codespan;
      return (
        <code key={key} className="rounded bg-hover px-1.5 py-0.5 font-mono text-xs text-fg">
          {codeSpan.text}
        </code>
      );
    }

    case 'code': {
      const codeToken = token as Tokens.Code;
      return (
        <pre
          key={key}
          className="my-2 max-w-full overflow-x-auto rounded-md border border-border bg-hover/70 p-3 font-mono text-xs text-fg"
          tabIndex={0}
        >
          <code>{codeToken.text}</code>
        </pre>
      );
    }

    case 'blockquote': {
      const bqToken = token as Tokens.Blockquote;
      return (
        <blockquote key={key} className="my-2 border-l-2 border-accent/60 pl-3 italic text-muted">
          {renderTokens(bqToken.tokens, `${key}-bq`)}
        </blockquote>
      );
    }

    case 'hr': {
      return <hr key={key} className="my-3 border-border" />;
    }

    case 'br': {
      return <br key={key} />;
    }

    case 'list': {
      const listToken = token as Tokens.List;
      const isTaskList = listToken.items.some((it) => it.task);
      const items = listToken.items.map((item, idx) => {
        const itemKey = `${key}-li-${idx}`;
        if (item.task) {
          const contentTokens = item.tokens?.filter((t) => t.type !== 'checkbox') ?? [];
          return (
            <li key={itemKey} className="flex items-start gap-2 my-1 list-none">
              <input
                type="checkbox"
                checked={!!item.checked}
                disabled
                readOnly
                aria-label={item.text}
                className="mt-1 h-3.5 w-3.5 shrink-0 rounded border-border text-accent accent-accent focus:ring-0 cursor-default"
              />
              <div className="min-w-0 flex-1">
                {renderTokens(contentTokens, `${itemKey}-c`)}
              </div>
            </li>
          );
        }
        return (
          <li key={itemKey} className="my-0.5">
            {renderTokens(item.tokens, `${itemKey}-c`)}
          </li>
        );
      });

      if (isTaskList) {
        return (
          <ul key={key} className="my-1.5 space-y-1 list-none pl-0 text-[14px]">
            {items}
          </ul>
        );
      }
      if (listToken.ordered) {
        return (
          <ol
            key={key}
            start={listToken.start ? Number(listToken.start) : undefined}
            className="my-1.5 ml-5 list-decimal space-y-0.5 text-[14px]"
          >
            {items}
          </ol>
        );
      }
      return (
        <ul key={key} className="my-1.5 ml-5 list-disc space-y-0.5 text-[14px]">
          {items}
        </ul>
      );
    }

    case 'link': {
      const linkToken = token as Tokens.Link;
      const safe = isSafeUrl(linkToken.href);
      const inner = renderTokens(linkToken.tokens, `${key}-link`);
      if (!safe) {
        return <span key={key}>{inner}</span>;
      }
      return (
        <a
          key={key}
          href={linkToken.href}
          target="_blank"
          rel="noreferrer noopener"
          className="text-accent underline hover:text-accent/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
        >
          {inner}
        </a>
      );
    }

    case 'image': {
      const imgToken = token as Tokens.Image;
      const label = imgToken.text || imgToken.href;
      return (
        <span key={key} className="text-muted italic select-none">
          [{label}]
        </span>
      );
    }

    case 'html': {
      const htmlToken = token as Tokens.HTML;
      // Raw HTML is never executed or rendered as markup; escaped as plain text.
      return <span key={key}>{htmlToken.raw}</span>;
    }

    case 'table': {
      const tableToken = token as Tokens.Table;
      return (
        <div key={key} className="my-2 overflow-x-auto">
          <table className="min-w-full divide-y divide-border border border-border text-xs">
            <thead>
              <tr>
                {tableToken.header.map((cell, cIdx) => (
                  <th
                    key={cIdx}
                    className="bg-hover/50 px-2.5 py-1.5 text-left font-semibold text-fg"
                  >
                    {renderTokens(cell.tokens, `${key}-th-${cIdx}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {tableToken.rows.map((row, rIdx) => (
                <tr key={rIdx}>
                  {row.map((cell, cIdx) => (
                    <td key={cIdx} className="px-2.5 py-1 text-fg">
                      {renderTokens(cell.tokens, `${key}-td-${rIdx}-${cIdx}`)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }

    case 'space':
      return null;

    default:
      if ('tokens' in token && Array.isArray((token as any).tokens)) {
        return <React.Fragment key={key}>{renderTokens((token as any).tokens, `${key}-unk`)}</React.Fragment>;
      }
      if ('text' in token && typeof (token as any).text === 'string') {
        return (token as any).text;
      }
      if ('raw' in token && typeof (token as any).raw === 'string') {
        return (token as any).raw;
      }
      return null;
  }
}

export function renderTokens(tokens: Token[] | undefined, prefix = 'tok'): React.ReactNode {
  if (!tokens || tokens.length === 0) return null;
  return tokens.map((token, idx) => renderToken(token, `${prefix}-${idx}`));
}

export function SafeMarkdown({
  content,
  className,
}: {
  content: string;
  className?: string;
}) {
  const parsed = useMemo(() => {
    if (!content || !content.trim()) return null;
    try {
      const tokens = marked.lexer(content, { gfm: true, breaks: true });
      return renderTokens(tokens, 'root');
    } catch {
      return content;
    }
  }, [content]);

  if (!parsed) return null;

  return (
    <div
      className={cn('safe-markdown leading-relaxed text-fg text-[14px]', className)}
      data-testid="safe-markdown"
    >
      {parsed}
    </div>
  );
}
