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
      { text: 'Integrations', link: '/integrations/cli' },
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
          { text: 'Non-systemd systems', link: '/guide/non-systemd' },
          { text: 'Quick Start', link: '/guide/quick-start' },
          { text: 'Recipes & Common Setups', link: '/guide/recipes' },
          { text: 'Upgrading & Uninstalling', link: '/guide/upgrading' },
          { text: 'Repair & Recovery', link: '/guide/recovery' }
        ]
      },
      {
        text: 'Architecture',
        collapsed: false,
        items: [
          { text: 'System Overview', link: '/architecture/overview' },
          { text: 'Go Core Daemon', link: '/architecture/core-engine' },
          { text: 'IPC Transport', link: '/architecture/ipc-transport' },
          { text: 'Security & Encryption', link: '/architecture/security' }
        ]
      },
      {
        text: 'TUI & Usage',
        collapsed: false,
        items: [
          { text: 'Navigation & Hotkeys', link: '/usage/navigation' },
          { text: 'Profiles & Groups', link: '/usage/profiles-groups' },
          { text: 'Traffic Monitoring', link: '/usage/traffic-monitoring' },
          { text: 'Log Tailer', link: '/usage/logs' },
          { text: 'Themes & Visuals', link: '/usage/themes' },
          { text: 'Settings Reference', link: '/usage/settings' }
        ]
      },
      {
        text: 'Networking & VPN',
        collapsed: false,
        items: [
          { text: 'TUN Mode & Capabilities', link: '/networking/tun-mode' },
          { text: 'DNS & FakeDNS', link: '/networking/dns' },
          { text: 'Split Tunneling (cgroup v2)', link: '/networking/split-tunnel' },
          { text: 'Kill-Switch Firewall', link: '/networking/killswitch' },
          { text: 'Routing Rules & Presets', link: '/networking/routing-rules' },
          { text: 'Happ Routing Profiles', link: '/networking/happ-routing' },
          { text: 'JSON Subscriptions', link: '/networking/json-subscriptions' },
          { text: 'Supported Protocols', link: '/networking/protocols' }
        ]
      },
      {
        text: 'Integrations & Ecosystem',
        collapsed: false,
        items: [
          { text: 'CLI Subcommands & Flags', link: '/integrations/cli' },
          { text: 'Desktop Widgets (Waybar/Polybar)', link: '/integrations/desktop-widgets' },
          { text: 'GNOME Shell Extension', link: '/integrations/gnome-extension' },
          { text: 'Deep Links (wit://)', link: '/integrations/deeplinks' },
          { text: 'Systemd User Service', link: '/integrations/systemd' },
          { text: 'HWID & Device Fingerprint', link: '/integrations/hwid' }
        ]
      },
      {
        text: 'Technical Reference',
        collapsed: false,
        items: [
          { text: 'TCP & IPC Wire Protocol', link: '/reference/protocol' },
          { text: 'Configuration Files', link: '/reference/configuration' },
          { text: 'Routing Precedence', link: '/reference/routing-precedence' },
          { text: 'Happ Format Reference', link: '/reference/happ-format' },
          { text: 'Doctor Diagnostics', link: '/reference/doctor' },
          { text: 'Troubleshooting & FAQ', link: '/reference/troubleshooting' },
          { text: 'Threat Model & Limitations', link: '/reference/threat-model' },
          { text: 'Reporting a Bug', link: '/reference/reporting' },
          { text: 'Development & Testing', link: '/reference/development' },
          { text: 'Glossary', link: '/reference/glossary' }
        ]
      },
      {
        text: 'Developers',
        collapsed: false,
        items: [
          { text: 'For Developers', link: '/developers/' },
          { text: 'Documentation Guide', link: '/developers/documentation' }
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
