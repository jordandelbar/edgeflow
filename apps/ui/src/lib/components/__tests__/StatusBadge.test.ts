import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import StatusBadge from '../StatusBadge.svelte';

describe('StatusBadge', () => {
  it.each(['FINISHED', 'RUNNING', 'FAILED', 'KILLED'])('renders %s text', (status) => {
    render(StatusBadge, { props: { status } });
    expect(screen.getByText(status)).toBeInTheDocument();
  });

  it('applies FINISHED styling', () => {
    const { container } = render(StatusBadge, { props: { status: 'FINISHED' } });
    expect(container.querySelector('span')?.className).toContain('text-sage-dark');
  });

  it('applies RUNNING styling', () => {
    const { container } = render(StatusBadge, { props: { status: 'RUNNING' } });
    expect(container.querySelector('span')?.className).toContain('text-peach-dark');
  });

  it('applies FAILED styling', () => {
    const { container } = render(StatusBadge, { props: { status: 'FAILED' } });
    expect(container.querySelector('span')?.className).toContain('text-red-700');
  });

  it('applies fallback styling for unknown status', () => {
    const { container } = render(StatusBadge, { props: { status: 'UNKNOWN' } });
    expect(container.querySelector('span')?.className).toContain('bg-gray-100');
  });
});
