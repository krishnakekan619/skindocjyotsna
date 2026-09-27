// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { rowActions } from './common';

afterEach(cleanup);

describe('rowActions (clickable rows usable from the keyboard)', () => {
  function renderRow(onOpen: () => void) {
    render(
      <div {...rowActions(onOpen)} data-testid="row">
        <button type="button">Edit</button>
      </div>,
    );
    return screen.getByTestId('row');
  }

  it('can be reached with Tab and is announced as a button', () => {
    const row = renderRow(() => undefined);
    expect(row.getAttribute('tabindex')).toBe('0');
    expect(row.getAttribute('role')).toBe('button');
  });

  it('opens on click, Enter and Space', () => {
    const onOpen = vi.fn();
    const row = renderRow(onOpen);
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: 'Enter' });
    fireEvent.keyDown(row, { key: ' ' });
    expect(onOpen).toHaveBeenCalledTimes(3);
  });

  it('ignores other keys and keys pressed on a button inside the row', () => {
    const onOpen = vi.fn();
    const row = renderRow(onOpen);
    fireEvent.keyDown(row, { key: 'a' });
    fireEvent.keyDown(screen.getByText('Edit'), { key: 'Enter' });
    expect(onOpen).not.toHaveBeenCalled();
  });
});
