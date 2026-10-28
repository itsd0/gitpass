import { Fragment, useMemo } from "react";
import { highlightSegments } from "./highlight";

export interface SearchHighlightProps {
  text: string;
  query: string;
}

/**
 * Renders `text` with every search match wrapped in a semantic `<mark>`.
 * With no query (or no match) it returns the plain string, so list rows keep
 * their exact DOM and text queries in tests and screen readers are unaffected.
 */
export function SearchHighlight({ text, query }: SearchHighlightProps) {
  const segments = useMemo(
    () => (query.trim() ? highlightSegments(text, query) : null),
    [text, query]
  );
  if (!segments || (segments.length === 1 && !segments[0].match)) return <>{text}</>;
  return (
    <>
      {segments.map((segment, i) =>
        segment.match ? (
          <mark
            key={i}
            data-testid="search-highlight"
            className="rounded-[2px] bg-accent/20 text-fg"
          >
            {segment.text}
          </mark>
        ) : (
          <Fragment key={i}>{segment.text}</Fragment>
        )
      )}
    </>
  );
}
