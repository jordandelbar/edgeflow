import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import ErrorCard from '../ErrorCard.svelte';

describe('ErrorCard', () => {
  it('renders the error message', () => {
    render(ErrorCard, { props: { message: 'Something went wrong' } });
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
  });

  it('renders the exclamation icon', () => {
    const { container } = render(ErrorCard, { props: { message: 'Error' } });
    expect(container.querySelector('.fa-circle-exclamation')).toBeInTheDocument();
  });

  it('applies error styling', () => {
    const { container } = render(ErrorCard, { props: { message: 'Error' } });
    const div = container.firstElementChild;
    expect(div?.className).toContain('text-red-600');
    expect(div?.className).toContain('bg-red-50');
  });
});
