import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import ExperimentCard from '../ExperimentCard.svelte';
import type { Experiment } from '$lib/api';

const mockExperiment: Experiment = {
  experiment_id: '42',
  name: 'Iris Classifier',
  artifact_location: 's3://bucket/42',
  lifecycle_stage: 'active',
  creation_time: new Date('2024-06-15').getTime(),
  last_update_time: new Date('2024-06-16').getTime(),
  tags: [],
};

describe('ExperimentCard', () => {
  it('renders the experiment name', () => {
    render(ExperimentCard, { props: { experiment: mockExperiment } });
    expect(screen.getByText('Iris Classifier')).toBeInTheDocument();
  });

  it('renders the experiment id', () => {
    render(ExperimentCard, { props: { experiment: mockExperiment } });
    expect(screen.getByText('#42')).toBeInTheDocument();
  });

  it('links to the correct experiment page', () => {
    render(ExperimentCard, { props: { experiment: mockExperiment } });
    const link = screen.getByRole('link');
    expect(link).toHaveAttribute('href', '/experiments/42');
  });

  it('renders a formatted creation date', () => {
    render(ExperimentCard, { props: { experiment: mockExperiment } });
    // Verify a date-like string is present (exact format depends on locale support in test env)
    const dateEl = screen.getByText(/2024|Jun/);
    expect(dateEl).toBeInTheDocument();
  });
});
