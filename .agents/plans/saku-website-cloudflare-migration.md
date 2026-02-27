# Plan: Saku Website — Starlight + Cloudflare Pages Migration

## Goal

Migrate the saku landing page from GitHub Pages (`docs/index.html`) to a Starlight (Astro) site hosted on Cloudflare Pages. The new site combines the custom landing page at `/` with full project documentation under `/docs/`, includes `llms.txt` for AI agent discoverability, and establishes documentation conventions for the ongoing development of the saku productivity suite.

## Context

### Current state

- **Landing page**: Self-contained `docs/index.html` (1,325 lines) — dark terminal-themed, embedded CSS/JS, served by GitHub Pages from the `/docs` folder on `main`
- **Documentation**: 10 markdown files in `documentation/` covering philosophy, architecture, tdo commands, sync, publishing, etc.
- **CI/CD**: `.github/workflows/rust.yml` — Rust build/test only, no deployment workflow
- **No custom domain**: Using default `github.io` subdomain
- **User already has a Cloudflare account** with other Pages projects

### Key files

- `docs/index.html` — Current landing page to port
- `documentation/PHILOSOPHY.md` — Design intent and vision
- `documentation/architecture.md` — Monorepo structure, patterns, conventions
- `documentation/tdo/commands-cheat-sheet.md` — Full tdo command reference
- `documentation/tdo/design-spec.md` — CLI visual design specification
- `documentation/sync-setup.md` — Sync configuration guide
- `documentation/publishing.md` — Crate publishing workflow
- `skills/README.md` — AI agent integration guide
- `.github/workflows/rust.yml` — Existing CI workflow

### Contracts

**URL routes:**
| Route | Content |
|---|---|
| `/` | Custom landing page (ported from current HTML) |
| `/docs/` | Starlight documentation root |
| `/docs/getting-started/` | Installation, quick start |
| `/docs/tdo/` | tdo command reference, design spec |
| `/docs/architecture/` | Architecture, philosophy |
| `/docs/guides/` | Sync setup, AI integration, publishing |
| `/llms.txt` | AI agent discovery index |
| `/llms-full.txt` | Full documentation for AI consumption |

**Content mapping:**
| Source file | Destination route |
|---|---|
| `documentation/PHILOSOPHY.md` | `/docs/philosophy/` |
| `documentation/architecture.md` | `/docs/architecture/` |
| `documentation/tdo/commands-cheat-sheet.md` | `/docs/tdo/commands/` |
| `documentation/tdo/design-spec.md` | `/docs/tdo/design-spec/` |
| `documentation/sync-setup.md` | `/docs/guides/sync-setup/` |
| `skills/README.md` | `/docs/guides/ai-integration/` |
| `documentation/publishing.md` | `/docs/guides/publishing/` |

**CI/CD:**
- GitHub Actions workflow: build Astro site → deploy to Cloudflare Pages
- Triggered on push to `main`

---

## Phases

### Phase 1: Scaffold Starlight project

**Description:** Create the Astro + Starlight project inside the repository with the basic structure, configuration, and a placeholder landing page. The site should build and run locally.

**To-do:**

- [ ] Create `site/` directory at the repo root
- [ ] Initialize Astro project with Starlight integration (`package.json`, `astro.config.mjs`, `tsconfig.json`)
- [ ] Configure Starlight in `astro.config.mjs`:
  - Site title: "Saku (作)"
  - Docs content directory pointing to `src/content/docs/`
  - Sidebar configuration matching the planned URL structure
  - Dark theme by default (matching the terminal aesthetic)
- [ ] Create `src/pages/index.astro` — minimal placeholder landing page (not yet ported)
- [ ] Create a placeholder `src/content/docs/index.md` as the docs homepage
- [ ] Add `.gitignore` entries for `site/node_modules/`, `site/dist/`, `site/.astro/`
- [ ] Verify `npm install && npm run dev` works and serves the site locally
- [ ] Verify `npm run build` produces static output in `site/dist/`

**Verification:**

- `cd site && npm run build` succeeds
- `npm run dev` serves the site at `localhost:4321`
- Navigating to `/` shows the placeholder landing page
- Navigating to `/docs/` shows the Starlight docs layout

---

### Phase 2: Port the landing page

**Description:** Convert the existing `docs/index.html` into `src/pages/index.astro`, preserving the terminal-themed design. The landing page uses its own layout (not Starlight's).

**To-do:**

- [ ] Create `src/layouts/LandingLayout.astro` — base HTML layout with the landing page's CSS custom properties, fonts (Spectral, IBM Plex Sans, JetBrains Mono), and meta tags
- [ ] Create `src/pages/index.astro` — port the landing page HTML structure into an Astro component using the landing layout
- [ ] Extract the CSS from the inline `<style>` block into `src/styles/landing.css`
- [ ] Port the JavaScript (copy-to-clipboard, scroll animations, reduced-motion support) into a `<script>` block or `src/scripts/landing.ts`
- [ ] Update all internal links:
  - GitHub links: verify they point to the correct repository URL
  - Documentation link in footer: point to `/docs/`
- [ ] Verify the landing page looks identical to the original `docs/index.html`
- [ ] Verify the "Documentation" link navigates to the Starlight docs

**Verification:**

- Visual comparison: landing page at `/` matches the current `docs/index.html` design
- All interactive elements work (copy buttons, scroll animations, reduced-motion)
- Internal links work correctly
- No console errors in the browser

---

### Phase 3: Migrate documentation content

**Description:** Move existing markdown documentation into Starlight's content collection, adapting frontmatter and structure. After this phase, all current docs are browsable at `/docs/`.

**To-do:**

- [ ] Create docs directory structure:
  ```
  site/src/content/docs/
  ├── index.md                    (docs homepage / overview)
  ├── getting-started.md          (installation + quick start, new content)
  ├── philosophy.md               (from documentation/PHILOSOPHY.md)
  ├── architecture.md             (from documentation/architecture.md)
  ├── tdo/
  │   ├── index.md                (tdo overview, new content)
  │   ├── commands.md             (from documentation/tdo/commands-cheat-sheet.md)
  │   └── design-spec.md          (from documentation/tdo/design-spec.md)
  └── guides/
      ├── sync-setup.md           (from documentation/sync-setup.md)
      ├── ai-integration.md       (from skills/README.md)
      └── publishing.md           (from documentation/publishing.md)
  ```
- [ ] Add Starlight frontmatter to each doc (title, description, sidebar position)
- [ ] Adapt internal links between docs to use relative Starlight paths
- [ ] Fix any markdown incompatibilities (Starlight uses MDX-compatible processing)
- [ ] Configure sidebar in `astro.config.mjs` to match the navigation structure:
  - Getting Started
  - Philosophy
  - Architecture
  - tdo (group: Overview, Commands, Design Spec)
  - Guides (group: Sync Setup, AI Integration, Publishing)
- [ ] Verify all docs render correctly with proper navigation

**Verification:**

- All doc pages render without errors
- Sidebar navigation matches the planned structure
- Internal links between docs work
- Code blocks, tables, and other markdown elements render correctly
- Search works across all documentation content

---

### Phase 4: Add llms.txt for AI agent discoverability

**Description:** Generate `llms.txt` and `llms-full.txt` files so AI coding agents can discover and consume saku's documentation. These are static files placed in the `public/` directory.

**To-do:**

- [ ] Create `site/public/llms.txt` following the llms.txt specification:
  - H1: "Saku (作)"
  - Blockquote summary
  - Sections with links to each doc page's `.md` equivalent or raw content
  - Optional section for less critical pages
- [ ] Create `site/public/llms-full.txt` — concatenation of all documentation content into a single markdown file
- [ ] Add a build script (`site/scripts/generate-llms-txt.mjs`) that:
  - Reads all `.md` files from `src/content/docs/`
  - Generates `llms.txt` (index with links)
  - Generates `llms-full.txt` (full concatenated content)
- [ ] Integrate the generation script into the build step (pre-build script in `package.json`)
- [ ] Verify both files are accessible at `/llms.txt` and `/llms-full.txt` after build

**Verification:**

- `curl http://localhost:4321/llms.txt` returns a valid llms.txt file
- `curl http://localhost:4321/llms-full.txt` returns concatenated documentation
- The llms.txt follows the specification (H1, blockquote, H2 sections with bullet links)
- Both files are included in the `dist/` output after `npm run build`

---

### Phase 5: CI/CD — GitHub Actions → Cloudflare Pages

**Description:** Set up automated deployment from GitHub to Cloudflare Pages. On push to `main`, the Astro site builds and deploys automatically. PR branches get preview deployments.

**To-do:**

- [ ] Create `.github/workflows/deploy-site.yml`:
  - Trigger on push to `main` and pull requests
  - Install Node.js dependencies in `site/`
  - Run the llms.txt generation script
  - Build the Astro site (`npm run build`)
  - Deploy to Cloudflare Pages using `cloudflare/wrangler-action`
  - Use GitHub secrets for `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
- [ ] Document the required GitHub secrets in the workflow file comments
- [ ] Add `site/wrangler.jsonc` (or configure in workflow) with:
  - Project name
  - Build output directory (`dist/`)
- [ ] Test the build step locally to verify it works end-to-end
- [ ] Remove the old `docs/index.html` (the GitHub Pages landing page) once Cloudflare Pages is live

**Verification:**

- The workflow YAML is valid (`actionlint` or similar)
- `cd site && npm run build` produces a complete static site in `dist/`
- The workflow references correct paths (`site/` working directory)
- The old `docs/index.html` is removed (after confirming Cloudflare Pages is live — this step is manual)

> **Note:** Actual Cloudflare Pages project creation, secret configuration, and DNS setup happen outside the codebase. The user will handle this manually in the Cloudflare dashboard since they already have an account.

---

### Phase 6: Documentation conventions — how to document going forward

**Description:** Create internal documentation that establishes how the saku team (human + agent) should write and maintain documentation from now on, and how the website fits into the development workflow.

**To-do:**

- [ ] Create `documentation/how-to-document.md` covering:
  - **Where docs live**: source markdown in `site/src/content/docs/`, internal-only docs stay in `documentation/`
  - **Public vs internal docs**: public docs go on the website; internal docs (RFCs, testing notes, WIP specs) stay in `documentation/`
  - **Adding a new doc page**: where to create the file, frontmatter format, how to add it to the sidebar, how it gets included in llms.txt
  - **Adding a new tool's docs**: pattern for creating a tool section (mirroring the `tdo/` structure)
  - **Writing style**: terminal-first audience, concise, code examples in every page, match the saku voice
  - **Images and assets**: where to put them (`site/src/assets/`), how to reference them
  - **Local development**: how to run the site locally (`cd site && npm run dev`)
  - **Deployment**: push to `main` triggers auto-deploy; preview deploys on PRs
  - **llms.txt maintenance**: auto-generated on build; no manual edits needed
  - **Updating the landing page**: edit `site/src/pages/index.astro`
- [ ] Update `documentation/architecture.md` to add a "Website" section referencing the new `site/` directory and `how-to-document.md`
- [ ] Update `README.md` to link to the live documentation site URL

**Verification:**

- `documentation/how-to-document.md` exists and covers all listed topics
- `documentation/architecture.md` references the website
- `README.md` links to the documentation site
- A new developer (or agent) can follow the guide to add a new doc page end-to-end

---

## Documentation to update once all phases are completed

- `documentation/how-to-document.md` — Created in Phase 6
- `documentation/architecture.md` — Updated in Phase 6 to reference website
- `README.md` — Updated in Phase 6 to link to live docs
- `CLAUDE.md` — Add a note about the website location (`site/`) and how to run it locally

## Next step

Start Phase 1: scaffold the Starlight project in `site/` with basic configuration and verify it builds locally.
