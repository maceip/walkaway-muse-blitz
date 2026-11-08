import {defineTheme} from '@astryxdesign/core/theme';
import {neutralTheme} from '@astryxdesign/theme-neutral';

export const walkawayGlassesTheme = defineTheme({
  name: 'walkaway-glasses',
  extends: neutralTheme,
  typography: {
    scale: {base: 16, ratio: 1.18},
    body: {
      family: '-apple-system',
      fallbacks:
        'BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
    },
    heading: {
      family: '-apple-system',
      fallbacks:
        'BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
      weights: {1: 'bold', 2: 'bold', 3: 'bold', 4: 'bold'},
    },
    code: {
      family: 'ui-monospace',
      fallbacks: '"SF Mono", Monaco, Consolas, monospace',
    },
  },
  tokens: {
    // Smoked-slate waveguide surface hierarchy (never pure #000000 inside cards)
    '--color-background-body': ['#181B20', '#181B20'],
    '--color-background-surface': ['#262A32', '#262A32'],
    '--color-background-card': ['#282C35', '#282C35'],
    '--color-background-popover': ['#343944', '#343944'],
    '--color-background-muted': ['#343944', '#343944'],

    // High-contrast waveguide text & primary pill tokens
    '--color-text-primary': ['#FFFFFF', '#FFFFFF'],
    '--color-text-secondary': ['#C4CAD4', '#C4CAD4'],
    '--color-text-disabled': ['#767E8C', '#767E8C'],
    '--color-accent': ['#FFFFFF', '#FFFFFF'],
    '--color-accent-muted': ['#3E4554', '#3E4554'],
    '--color-on-accent': ['#111317', '#111317'],
    '--color-text-accent': ['#38BDF8', '#38BDF8'],
    '--color-icon-accent': ['#38BDF8', '#38BDF8'],

    // Borders and focus indicator tuned for waveguide legibility
    '--color-border': ['#4A505E', '#4A505E'],
    '--color-border-emphasized': ['#6E7681', '#6E7681'],
    '--focus-outline-width': '3px',
    '--focus-outline-color': '#38BDF8',
    '--focus-outline-offset': '3px',

    // Muted semantic status fills (Threads Report crimson #7D242B)
    '--color-info': ['#1D4ED8', '#2563EB'],
    '--color-on-info': ['#FFFFFF', '#FFFFFF'],
    '--color-success': ['#15803D', '#16A34A'],
    '--color-on-success': ['#FFFFFF', '#FFFFFF'],
    '--color-warning': ['#B45309', '#D97706'],
    '--color-on-warning': ['#FFFFFF', '#FFFFFF'],
    '--color-error': ['#7D242B', '#7D242B'],
    '--color-on-error': ['#FFFFFF', '#FFFFFF'],
    '--color-error-muted': ['#7D242B', '#7D242B'],
    '--color-on-error-muted': ['#FFFFFF', '#FFFFFF'],

    // Rounded containers and pill controls matching Threads/WhatsApp glasses UI
    '--radius-inner': '12px',
    '--radius-element': '9999px',
    '--radius-container': '28px',
    '--radius-page': '32px',
    '--size-element-sm': '40px',
    '--size-element-md': '46px',
    '--size-element-lg': '52px',
  },
  components: {
    card: {
      base: {
        padding: 'var(--spacing-4)',
        borderRadius: '28px',
      },
    },
    button: {
      base: {
        borderRadius: '9999px',
      },
      'variant:destructive': {
        backgroundColor: 'var(--color-error)',
        color: 'var(--color-on-error)',
      },
    },
    'segmented-control': {
      base: {
        padding: 'var(--spacing-1)',
        borderRadius: '9999px',
      },
    },
  },
});
