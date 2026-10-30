import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useI18n } from '../../i18n';
import { ScoreHistory } from './ScoreHistory';
import type { ScorePoint } from './history';

const now = new Date('2026-10-30T12:00:00');
const pt = (date: string, score: number): ScorePoint => ({ date, score, counts: {} });
afterEach(() => { cleanup(); useI18n.getState().setLang('en'); });

describe('ScoreHistory', () => {
  it('shows the empty state with fewer than 2 points', () => {
    render(<ScoreHistory points={[pt('2026-10-30', 70)]} now={now} />);
    expect(screen.getByTestId('score-history-empty').textContent).toBe('Your score history will appear here after a few days.');
    expect(screen.queryByTestId('score-history-chart')).toBeNull();
  });

  it('shows a positive weekly delta and an accessible summary', () => {
    render(<ScoreHistory points={[pt('2026-10-20', 72), pt('2026-10-23', 70), pt('2026-10-30', 76)]} now={now} />);
    expect(screen.getByTestId('score-history-delta').textContent).toBe('+6 since last week');
    expect(screen.getByTestId('score-history-delta').className).toContain('text-ok');
    expect(screen.getByRole('group', { name: 'Score went from 72 to 76 over 30 days' })).toBeTruthy();
  });

  it('shows a negative delta and no-change text', () => {
    const { unmount } = render(<ScoreHistory points={[pt('2026-10-22', 80), pt('2026-10-30', 76)]} now={now} />);
    expect(screen.getByTestId('score-history-delta').textContent).toBe('−4 since last week');
    expect(screen.getByTestId('score-history-delta').className).toContain('text-danger');
    unmount();
    render(<ScoreHistory points={[pt('2026-10-22', 76), pt('2026-10-30', 76)]} now={now} />);
    expect(screen.getByTestId('score-history-delta').textContent).toBe('No change');
  });

  it('moves between points with the arrow keys', () => {
    render(<ScoreHistory points={[pt('2026-10-20', 72), pt('2026-10-23', 70), pt('2026-10-30', 76)]} now={now} />);
    const chart = screen.getByTestId('score-history-chart');
    expect(screen.getByTestId('score-history-tooltip').textContent).toBe('Oct 30 · score 76');
    chart.focus();
    fireEvent.keyDown(chart, { key: 'ArrowLeft' });
    expect(screen.getByTestId('score-history-tooltip').textContent).toBe('Oct 23 · score 70');
    fireEvent.keyDown(chart, { key: 'Home' });
    expect(screen.getByTestId('score-history-tooltip').textContent).toBe('Oct 20 · score 72');
    fireEvent.keyDown(chart, { key: 'ArrowLeft' });
    expect(screen.getByTestId('score-history-tooltip').textContent).toBe('Oct 20 · score 72');
    fireEvent.keyDown(chart, { key: 'End' });
    expect(screen.getByTestId('score-history-tooltip').textContent).toBe('Oct 30 · score 76');
  });

  it('toggles 30/90 days', () => {
    const pts = [pt('2026-08-15', 50), pt('2026-10-20', 72), pt('2026-10-30', 76)];
    render(<ScoreHistory points={pts} now={now} />);
    expect(screen.getByRole('group', { name: 'Score went from 72 to 76 over 30 days' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '90 days' }));
    expect(screen.getByRole('group', { name: 'Score went from 50 to 76 over 90 days' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '90 days' }).getAttribute('aria-pressed')).toBe('true');
  });

  it('is localized in Burmese and Thai', () => {
    useI18n.getState().setLang('th');
    render(<ScoreHistory points={[pt('2026-10-20', 72), pt('2026-10-30', 76)]} now={now} />);
    expect(screen.getByText('ประวัติคะแนน')).toBeTruthy();
    expect(screen.getByTestId('score-history-delta').textContent).toBe('+4 จากสัปดาห์ที่แล้ว');
    cleanup();
    useI18n.getState().setLang('my');
    render(<ScoreHistory points={[pt('2026-10-20', 72), pt('2026-10-30', 76)]} now={now} />);
    expect(screen.getByTestId('score-history-delta').textContent).toBe('ပြီးခဲ့သည့်အပတ်မှ +4');
  });
});
