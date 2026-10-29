import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SafeMarkdown, isSafeUrl } from './SafeMarkdown';

describe('SafeMarkdown unit & safety tests', () => {
  it('renders headings from h1 to h6 with correct styling classes', () => {
    const md = `# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4\n##### Heading 5\n###### Heading 6`;
    render(<SafeMarkdown content={md} />);

    const h1 = screen.getByRole('heading', { level: 1 });
    expect(h1).toHaveTextContent('Heading 1');
    expect(h1.className).toContain('text-xl');

    const h2 = screen.getByRole('heading', { level: 2 });
    expect(h2).toHaveTextContent('Heading 2');
    expect(h2.className).toContain('text-lg');

    const h3 = screen.getByRole('heading', { level: 3 });
    expect(h3).toHaveTextContent('Heading 3');
    expect(h3.className).toContain('text-base');

    const h4 = screen.getByRole('heading', { level: 4 });
    expect(h4).toHaveTextContent('Heading 4');

    const h5 = screen.getByRole('heading', { level: 5 });
    expect(h5).toHaveTextContent('Heading 5');

    const h6 = screen.getByRole('heading', { level: 6 });
    expect(h6).toHaveTextContent('Heading 6');
  });

  it('renders bold, italic, strikethrough and inline code', () => {
    const md = `This is **bold text**, *italic text*, ~~struck text~~, and \`const a = 42\`.`;
    render(<SafeMarkdown content={md} />);

    expect(screen.getByText('bold text').tagName.toLowerCase()).toBe('strong');
    expect(screen.getByText('italic text').tagName.toLowerCase()).toBe('em');
    expect(screen.getByText('struck text').tagName.toLowerCase()).toBe('del');

    const code = screen.getByText('const a = 42');
    expect(code.tagName.toLowerCase()).toBe('code');
    expect(code.className).toContain('font-mono');
  });

  it('renders fenced code blocks with horizontal scrolling', () => {
    const md = "```typescript\nfunction test() {\n  return 123;\n}\n```";
    render(<SafeMarkdown content={md} />);

    const pre = screen.getByText(/function test\(\)/).closest('pre');
    expect(pre).toBeTruthy();
    expect(pre?.className).toContain('overflow-x-auto');
    expect(pre?.className).toContain('font-mono');
  });

  it('renders bullet lists and numbered lists', () => {
    const md = `- Apple\n- Banana\n- Cherry\n\n1. First\n2. Second`;
    render(<SafeMarkdown content={md} />);

    expect(screen.getByText('Apple').tagName.toLowerCase()).toBe('li');
    expect(screen.getByText('Banana').tagName.toLowerCase()).toBe('li');
    expect(screen.getByText('First').tagName.toLowerCase()).toBe('li');
    expect(screen.getByText('Second').tagName.toLowerCase()).toBe('li');

    const lists = screen.getAllByRole('list');
    expect(lists.length).toBeGreaterThanOrEqual(2);
    expect(lists[0].tagName.toLowerCase()).toBe('ul');
    expect(lists[1].tagName.toLowerCase()).toBe('ol');
  });

  it('renders task lists with read-only checkboxes', () => {
    const md = `- [ ] Pending item\n- [x] Finished item`;
    render(<SafeMarkdown content={md} />);

    const checkboxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
    expect(checkboxes).toHaveLength(2);

    expect(checkboxes[0].checked).toBe(false);
    expect(checkboxes[0].disabled).toBe(true);
    expect(checkboxes[0].readOnly).toBe(true);

    expect(checkboxes[1].checked).toBe(true);
    expect(checkboxes[1].disabled).toBe(true);
    expect(checkboxes[1].readOnly).toBe(true);

    expect(screen.getByText('Pending item')).toBeInTheDocument();
    expect(screen.getByText('Finished item')).toBeInTheDocument();
  });

  it('renders blockquotes and horizontal rules', () => {
    const md = `> This is a quote\n\n---`;
    render(<SafeMarkdown content={md} />);

    const bq = screen.getByText('This is a quote').closest('blockquote');
    expect(bq).toBeTruthy();
    expect(bq?.className).toContain('border-l-2');

    const hr = document.querySelector('hr');
    expect(hr).toBeTruthy();
  });

  it('validates safe URLs vs unsafe protocols with isSafeUrl', () => {
    expect(isSafeUrl('https://example.com')).toBe(true);
    expect(isSafeUrl('http://example.com/test')).toBe(true);
    expect(isSafeUrl('mailto:user@example.com')).toBe(true);
    expect(isSafeUrl('javascript:alert(1)')).toBe(false);
    expect(isSafeUrl('JAVASCRIPT:alert(document.cookie)')).toBe(false);
    expect(isSafeUrl('data:text/html,<script>alert(1)</script>')).toBe(false);
    expect(isSafeUrl('file:///etc/passwd')).toBe(false);
    expect(isSafeUrl('')).toBe(false);
    expect(isSafeUrl(null)).toBe(false);
  });

  it('renders safe http/https/mailto links with target="_blank" and focus styles', () => {
    const md = `[Visit Site](https://example.com/login) and [Email](mailto:help@example.com)`;
    render(<SafeMarkdown content={md} />);

    const link = screen.getByRole('link', { name: 'Visit Site' }) as HTMLAnchorElement;
    expect(link.href).toBe('https://example.com/login');
    expect(link.target).toBe('_blank');
    expect(link.rel).toBe('noreferrer noopener');
    expect(link.className).toContain('text-accent');
    expect(link.className).toContain('focus-visible:ring-accent');

    const mailLink = screen.getByRole('link', { name: 'Email' }) as HTMLAnchorElement;
    expect(mailLink.href).toBe('mailto:help@example.com');
  });

  it('renders javascript: links inert as plain text, not clickable links', () => {
    const md = `[Click Me](javascript:alert(document.cookie))`;
    render(<SafeMarkdown content={md} />);

    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.getByText('Click Me')).toBeInTheDocument();
    expect(screen.getByText('Click Me').tagName.toLowerCase()).not.toBe('a');
  });

  it('renders raw HTML such as <script> and <img onerror> inert as text', () => {
    const md = `Hello <script>alert("xss")</script> and <img src="x" onerror="alert(1)" /> world`;
    render(<SafeMarkdown content={md} />);

    expect(document.querySelector('script')).toBeNull();
    expect(document.querySelector('img')).toBeNull();
    expect(document.body.textContent).toContain('<script>alert("xss")</script>');
    expect(screen.getByText(/<img src="x" onerror="alert\(1\)" \/>/)).toBeInTheDocument();
  });

  it('does NOT create img elements or make network requests for markdown images', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const md = `![A cute cat](https://example.com/cat.png)`;
    render(<SafeMarkdown content={md} />);

    expect(document.querySelector('img')).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(screen.getByText('[A cute cat]')).toBeInTheDocument();
  });

  it('handles empty or whitespace-only content cleanly', () => {
    const { container, rerender } = render(<SafeMarkdown content="" />);
    expect(container.firstChild).toBeNull();

    rerender(<SafeMarkdown content={"   \n  \t"} />);
    expect(container.firstChild).toBeNull();
  });
});
