// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://rapidity-rs.github.io',
  base: '/lets',
  integrations: [
    starlight({
      title: 'lets',
      description:
        'A declarative CLI builder. Define commands in KDL, get a production-quality CLI instantly.',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/rapidity-rs/lets' },
      ],
      sidebar: [
        {
          label: 'Start here',
          items: ['getting-started', 'kdl-primer'],
        },
        {
          label: 'Guides',
          items: [
            'commands',
            'arguments-and-flags',
            'orchestration',
            'environment',
            'interactive',
            'advanced',
          ],
        },
        {
          label: 'Reference',
          items: ['reference', 'shell-integration'],
        },
      ],
    }),
  ],
});
