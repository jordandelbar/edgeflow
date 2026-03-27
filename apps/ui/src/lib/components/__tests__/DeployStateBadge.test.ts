import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import DeployStateBadge from '../DeployStateBadge.svelte';

describe('DeployStateBadge', () => {
  it.each(['pending', 'deploying', 'upgrading', 'deployed', 'failed', 'superseded'])(
    'renders %s text',
    (state) => {
      render(DeployStateBadge, { props: { state } });
      expect(screen.getByText(state)).toBeInTheDocument();
    }
  );

  it('applies deployed styling', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'deployed' } });
    expect(container.querySelector('span')?.className).toContain('text-sage-dark');
  });

  it('applies failed styling', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'failed' } });
    expect(container.querySelector('span')?.className).toContain('text-red-600');
  });

  it('applies fallback styling for unknown state', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'unknown' } });
    expect(container.querySelector('span')?.className).toContain('bg-gray-100');
  });

  it('renders the icon for deployed state', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'deployed' } });
    expect(container.querySelector('.fa-circle-check')).toBeInTheDocument();
  });

  it('renders the icon for failed state', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'failed' } });
    expect(container.querySelector('.fa-circle-xmark')).toBeInTheDocument();
  });

  it('renders fallback icon for unknown state', () => {
    const { container } = render(DeployStateBadge, { props: { state: 'mystery' } });
    expect(container.querySelector('.fa-circle')).toBeInTheDocument();
  });
});
