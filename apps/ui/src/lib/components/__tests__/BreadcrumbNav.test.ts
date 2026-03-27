import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import BreadcrumbNav from '../BreadcrumbNav.svelte';

describe('BreadcrumbNav', () => {
  it('renders item labels', () => {
    render(BreadcrumbNav, {
      props: { items: [{ label: 'Experiments', href: '/' }, { label: 'My Experiment' }] },
    });
    expect(screen.getByText('Experiments')).toBeInTheDocument();
    expect(screen.getByText('My Experiment')).toBeInTheDocument();
  });

  it('renders items with href as links', () => {
    render(BreadcrumbNav, {
      props: { items: [{ label: 'Experiments', href: '/' }] },
    });
    const link = screen.getByRole('link', { name: 'Experiments' });
    expect(link).toHaveAttribute('href', '/');
  });

  it('renders items without href as plain text (no anchor)', () => {
    const { container } = render(BreadcrumbNav, {
      props: { items: [{ label: 'Current Page' }] },
    });
    expect(screen.queryByRole('link', { name: 'Current Page' })).toBeNull();
    expect(screen.getByText('Current Page')).toBeInTheDocument();
    expect(container.querySelector('span.font-medium')).toBeInTheDocument();
  });

  it('renders separators between items', () => {
    const { container } = render(BreadcrumbNav, {
      props: {
        items: [
          { label: 'Experiments', href: '/' },
          { label: 'Experiment 1', href: '/experiments/1' },
          { label: 'Run A' },
        ],
      },
    });
    const chevrons = container.querySelectorAll('.fa-chevron-right');
    expect(chevrons).toHaveLength(2);
  });

  it('renders no separator for a single item', () => {
    const { container } = render(BreadcrumbNav, {
      props: { items: [{ label: 'Only Item' }] },
    });
    expect(container.querySelector('.fa-chevron-right')).toBeNull();
  });
});
