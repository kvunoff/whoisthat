import { readFileSync } from 'node:fs'
import { defineConfig } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

function getClientVersion(): string {
  try {
    const cargoPath = new URL('../../Cargo.toml', import.meta.url)
    const content = readFileSync(cargoPath, 'utf-8')
    const match = content.match(/^version\s*=\s*"([^"]+)"/m)
    if (match) return match[1]
  } catch {}
  try {
    const pkgPath = new URL('../../package.json', import.meta.url)
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'))
    if (pkg.version) return pkg.version
  } catch {}
  throw new Error(
    'cannot determine the client version: read Cargo.toml or package.json from the repo root'
  )
}

const clientVersion = getClientVersion()

export default withMermaid(defineConfig({
  title: 'WhoisThat',
  description: 'Modern terminal-based VPN client for Linux. Rust TUI frontend with a Go engine backed by Xray-core.',
  base: '/',
  cleanUrls: true,
  appearance: 'dark',
  lastUpdated: true,

  sitemap: {
    hostname: 'https://docs.tuqise.com'
  },

  head: [
    ['meta', { name: 'theme-color', content: '#0f0f11' }],
    ['meta', { name: 'apple-mobile-web-app-capable', content: 'yes' }],
    ['meta', { name: 'apple-mobile-web-app-status-bar-style', content: 'black' }],
    ['meta', { property: 'og:site_name', content: 'WhoisThat Documentation' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:url', content: 'https://docs.tuqise.com/' }],
    ['meta', { property: 'og:title', content: 'WhoisThat — Modern Terminal VPN Client for Linux' }],
    ['meta', { property: 'og:description', content: 'Rust TUI frontend paired with an autonomous Go engine backed by Xray-core. Native TUN mode, split tunneling, and encrypted state.' }],
    ['meta', { property: 'og:image', content: 'https://docs.tuqise.com/logo.svg' }],
    ['meta', { name: 'twitter:card', content: 'summary' }],
    ['meta', { name: 'twitter:title', content: 'WhoisThat — Modern Terminal VPN Client for Linux' }],
    ['meta', { name: 'twitter:description', content: 'Rust TUI frontend paired with an autonomous Go engine backed by Xray-core.' }],
    ['meta', { name: 'twitter:image', content: 'https://docs.tuqise.com/logo.svg' }]
  ],

  themeConfig: {
    siteTitle: 'WhoisThat',
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/guide/quick-start' },
      { text: 'Architecture', link: '/architecture/overview' },
      { text: 'Networking', link: '/networking/tun-mode' },
      { text: 'Integrations', link: '/integrations/desktop-widgets' },
      { text: 'API & Wire Protocol', link: '/reference/protocol' },
      { text: 'Developers', link: '/developers/' },
      {
        text: `v${clientVersion}`,
        items: [
          { text: 'Changelog / Releases', link: 'https://github.com/kvunoff/whoisthat/releases' },
          { text: 'AUR (prebuilt)', link: 'https://aur.archlinux.org/packages/whoisthat-bin' },
          { text: 'AUR (source)', link: 'https://aur.archlinux.org/packages/whoisthat' },
          { text: 'GNOME Extension', link: 'https://github.com/kvunoff/whoisthat-extension' }
        ]
      }
    ],

    sidebar: [
      {
        text: 'Getting Started',
        collapsed: false,
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Installation', link: '/guide/installation' },
          { text: 'Quick Start', link: '/guide/quick-start' },
          { text: 'Platforms & Service Management', link: '/guide/platforms' },
          { text: 'Maintenance, Upgrading & Recovery', link: '/guide/maintenance' }
        ]
      },
      {
        text: 'Usage',
        collapsed: false,
        items: [
          { text: 'Navigation & Keybindings', link: '/usage/navigation' },
          { text: 'Profiles & Subscriptions', link: '/usage/profiles' },
          { text: 'Routing', link: '/usage/routing' },
          { text: 'Split Tunneling', link: '/usage/split-tunnel' },
          { text: 'Kill-Switch', link: '/usage/kill-switch' },
          { text: 'Traffic Monitoring', link: '/usage/traffic' },
          { text: 'Logs', link: '/usage/logs' },
          { text: 'Settings & Appearance', link: '/usage/settings' },
          { text: 'Recipes & Common Setups', link: '/usage/recipes' }
        ]
      },
      {
        text: 'Networking',
        collapsed: false,
        items: [
          { text: 'TUN Mode & Capabilities', link: '/networking/tun-mode' },
          { text: 'DNS & FakeDNS', link: '/networking/dns' },
          { text: 'Routing Rules', link: '/networking/routing-rules' },
          { text: 'Happ Routing Profiles', link: '/networking/happ-routing' },
          { text: 'JSON Subscriptions', link: '/networking/json-subscriptions' },
          { text: 'Supported Protocols', link: '/networking/protocols' }
        ]
      },
      {
        text: 'Integrations',
        collapsed: false,
        items: [
          { text: 'Desktop Widgets (Waybar/Polybar)', link: '/integrations/desktop-widgets' },
          { text: 'GNOME Shell Extension', link: '/integrations/gnome-extension' },
          { text: 'Deep Links (wit://)', link: '/integrations/deeplinks' },
          { text: 'HWID & Device Fingerprint', link: '/integrations/hwid' }
        ]
      },
      {
        text: 'Reference',
        collapsed: false,
        items: [
          { text: 'CLI Reference', link: '/reference/cli' },
          { text: 'Configuration', link: '/reference/configuration' },
          { text: 'Status JSON', link: '/reference/status-json' },
          { text: 'Routing Precedence', link: '/reference/routing-precedence' },
          { text: 'Happ Format', link: '/reference/happ-format' },
          { text: 'IPC & Wire Protocol', link: '/reference/protocol' },
          { text: 'Doctor Diagnostics', link: '/reference/doctor' },
          { text: 'Troubleshooting & FAQ', link: '/reference/troubleshooting' },
          { text: 'Glossary', link: '/reference/glossary' }
        ]
      },
      {
        text: 'Architecture',
        collapsed: false,
        items: [
          { text: 'System Overview', link: '/architecture/overview' },
          { text: 'Rust Client', link: '/architecture/rust-client' },
          { text: 'Go Core Daemon', link: '/architecture/go-core' },
          { text: 'IPC & Event Flow', link: '/architecture/ipc-flow' },
          { text: 'Data & Encryption', link: '/architecture/data-and-encryption' },
          { text: 'Kernel Networking', link: '/architecture/networking' },
          { text: 'Routing Engine', link: '/architecture/routing-engine' },
          { text: 'Xray Runtime', link: '/architecture/xray-runtime' },
          { text: 'Security & Threat Model', link: '/architecture/security' }
        ]
      },
      {
        text: 'Developers',
        collapsed: false,
        items: [
          { text: 'For Developers', link: '/developers/' },
          { text: 'Documentation Guide', link: '/developers/documentation' },
          { text: 'Build & Run', link: '/developers/build' },
          { text: 'Repository Map', link: '/developers/repository' },
          { text: 'Testing', link: '/developers/testing' },
          { text: 'Debugging', link: '/developers/debugging' },
          { text: 'Releasing', link: '/developers/releasing' }
        ]
      }
    ],

    search: {
      provider: 'local'
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/kvunoff/whoisthat' }
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2024–2026 kvunoff & WhoisThat Contributors'
    },

    editLink: {
      pattern: 'https://github.com/kvunoff/whoisthat/edit/main/docs/:path',
      text: 'Edit this page on GitHub'
    }
  }
}))
