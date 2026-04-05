# Changelog

## [0.6.0](https://github.com/h-michael/kabu/compare/v0.5.0..v0.6.0) - 2026-04-05


- **feat**: Add rm alias for remove command - ([74fc4f2](https://github.com/h-michael/kabu/commit/74fc4f29c74f4936e53f709f91a90e18ac36ca4b))
- **docs**: Update README.md - ([5c01eed](https://github.com/h-michael/kabu/commit/5c01eed0bbb9a8dbc324c5129f2b63d2bc3e513d))
- **feat**: Improve UI and messages - ([29e0b4d](https://github.com/h-michael/kabu/commit/29e0b4dfd49d5416f79958c1ed2429a3fa9e5854))
- **feat**: Introduce GitHub Actions Cache - ([469eabf](https://github.com/h-michael/kabu/commit/469eabf17d71ff66d910171812435593a78a7e8a))
- **feat**: [**breaking**] Rename config keys for clarity - ([0b14c15](https://github.com/h-michael/kabu/commit/0b14c15f5e6b22edb897f94dc9ab56b5b04580db))
- **fix**: Correctly handle non-origin remote names - ([a4eb4cb](https://github.com/h-michael/kabu/commit/a4eb4cb54783ceb72e7cd6bdb03d4ba5c7e48008))
- **docs**: Enhance CLI help, README, and configuration consistency - ([12e4478](https://github.com/h-michael/kabu/commit/12e4478ff41c071cae2b1dcb046a8ec6faeeced2))
- **feat**: Add shell integration and CD commands with improved trust system - ([c500ca6](https://github.com/h-michael/kabu/commit/c500ca6f87fe77e0571e8097306ffb8af9757ca1))
- **feat**: Implement trust system with full config tracking and diff display - ([e184bd4](https://github.com/h-michael/kabu/commit/e184bd4f2be7d794e3f5f2b27c6e4cb4623e1317))
- **chore**(ci): Bump EmbarkStudios/cargo-deny-action from 2.0.14 to 2.0.15 - ([a040112](https://github.com/h-michael/kabu/commit/a040112edd09f950296a41a5dbb62133bf0258ba))
- **refactor**: Unify dry-run hook output - ([a82aae3](https://github.com/h-michael/kabu/commit/a82aae3e83c66c5459111a10f56d1f72cdc78adb))
- **docs**: Fix config extension wording - ([d974a0f](https://github.com/h-michael/kabu/commit/d974a0fa5957b0a74840cd4e50ddfc88a5ebb683))
- **refactor**: Centralize trust checks and worktree removal - ([7f7e8d7](https://github.com/h-michael/kabu/commit/7f7e8d7029fbe8ab11b50375688e5e8bd24269ec))
- **feat**: Migrate interactive UI to ratatui - ([2decc58](https://github.com/h-michael/kabu/commit/2decc58d5b22eb6bae058c8ce8c4e70591b59b1f))
- **feat**: Add Windows hook shell selection - ([7cb0a40](https://github.com/h-michael/kabu/commit/7cb0a40c7759457fcd13e63de5c698dbb6dd01cd))
- **chore**(ci): Bump actions/checkout from 6.0.1 to 6.0.2 - ([3b5f624](https://github.com/h-michael/kabu/commit/3b5f624b27baa2e4b4e4455ff8bc9422217b3871))
- **chore**(ci): Bump actions/cache from 5.0.1 to 5.0.2 - ([0c1ee44](https://github.com/h-michael/kabu/commit/0c1ee440f968fdd173fd594a9fbc482896dcb3a4))
- **feat**: Add config new and global config - ([0546b56](https://github.com/h-michael/kabu/commit/0546b56b6e5e6dc2d64a47759974c30d352b3b66))
- **refactor**: Move interactive UI logic to submodules - ([2979641](https://github.com/h-michael/kabu/commit/2979641374544c8041e0b0faf3f91b6cf88573ec))
- **feat**: Add auto_cd configuration and rename switch to cd - ([cceff13](https://github.com/h-michael/kabu/commit/cceff135643e17abbafde3a122fbc55dfcd007dd))
- **docs**: Update documentation for auto_cd and config changes - ([e86a667](https://github.com/h-michael/kabu/commit/e86a667ba293ce505aaac66453c00609aa690db6))
- **chore**(ci): Bump docker/setup-buildx-action from 3.0.0 to 3.12.0 - ([5e7499e](https://github.com/h-michael/kabu/commit/5e7499e8b1df96eaabcccdf831630688906fe45c))
- **chore**(ci): Bump actions/cache from 5.0.2 to 5.0.3 - ([20a89fd](https://github.com/h-michael/kabu/commit/20a89fdd0cc93c563674877a02002951dd6258e5))
- **feat**: Add VCS abstraction layer for git/jj support - ([cdccfaa](https://github.com/h-michael/kabu/commit/cdccfaaad5ccfaa1c9e11e93c36b0ce681cfc6e6))
- **feat**: Implement jj workspace provider - ([3c06eb1](https://github.com/h-michael/kabu/commit/3c06eb1b05f5cfa9af98ecd84aaf1a28185f5321))
- **refactor**: Update commands to use VCS provider abstraction - ([b677133](https://github.com/h-michael/kabu/commit/b6771334025a977e8ac24a3e3c9aefa705ec7729))
- **docs**: Update documentation and CLI help for jj support - ([a879f01](https://github.com/h-michael/kabu/commit/a879f017ea6175d333f5104da92d48149d6b2eab))
- **refactor**: Cleanup unused code and update module structure - ([88523f0](https://github.com/h-michael/kabu/commit/88523f05d7b3673a9a9c361bac9032c75b64afd2))
- **docs**: Add template variable documentation to repo_config_template - ([907d0b8](https://github.com/h-michael/kabu/commit/907d0b854e0bae7437162a61f56493a5afeae73c))
- **fix**: Clear state when navigating back in interactive add flow - ([c24b9d7](https://github.com/h-michael/kabu/commit/c24b9d7957a3a09a5bd16573b01853d65d3c837f))
- **feat**: Add help modal to interactive flows - ([6d1eb15](https://github.com/h-michael/kabu/commit/6d1eb1533279e4402af0f9e2e93806fd02474da9))
- **feat**: [**breaking**] Migrate config path from .gwtx.yaml to .gwtx/config.yaml - ([d5c186d](https://github.com/h-michael/kabu/commit/d5c186dda8ab7d534b9bc68eed86d7e2f352a653))
- **feat**: [**breaking**] Rename CLI from gwtx to kabu - ([65e185b](https://github.com/h-michael/kabu/commit/65e185b13e5b9cb24e47185b13ae2015e777fe1e))
- **refactor**(interactive): Simplify search input and rename log to history - ([eb5b412](https://github.com/h-michael/kabu/commit/eb5b41268103387db37d301ae62fdd923963d766))
- **refactor**(interactive): Extract common layout into UiLayout struct - ([1efdf6d](https://github.com/h-michael/kabu/commit/1efdf6d2d5110ac1ff66211b6b5922b29e772b95))
- **feat**(config): Add TOML configuration format support - ([e4a2ce7](https://github.com/h-michael/kabu/commit/e4a2ce7caf9bbe4dbc53627c81280217dd23c1a4))
- **feat**(hook): Provide environment variables to hooks - ([a310b3a](https://github.com/h-michael/kabu/commit/a310b3a83f48563b026b7b3fd8956be36bdefb07))
- **feat**(config): Add #:schema directive to TOML templates - ([163fd43](https://github.com/h-michael/kabu/commit/163fd4319bb3679957110c0f0e586135b7d68277))
- **fix**(hook): Rename GWTHOOK_SHELL to KABUHOOK_SHELL - ([bbe773b](https://github.com/h-michael/kabu/commit/bbe773bbe3910fba8b3379981ed52db0ea9f6b1a))
- **fix**(interactive): Preserve Windows drive prefix in normalize_path - ([2c74f04](https://github.com/h-michael/kabu/commit/2c74f0438492f2f921d0587bdada3e8445f829f4))
- **docs**(examples): Update hooks platform support description - ([6f57628](https://github.com/h-michael/kabu/commit/6f57628af9d7e3a1efe050cfc42e9033fd659335))
- **fix**(config): Reject Windows drive-relative paths in validation - ([ef8b1db](https://github.com/h-michael/kabu/commit/ef8b1db5b2c00d7095d87fbd7b20eb621f907163))
- **chore**(ci): Bump taiki-e/upload-rust-binary-action from 1.27.0 to 1.28.0 - ([ea2f91c](https://github.com/h-michael/kabu/commit/ea2f91c87bcb162be4c6f38a004b97519ad65bd8))
- **chore**(deps): Update Cargo.lock - ([3d97aeb](https://github.com/h-michael/kabu/commit/3d97aebe1e0dfe5530e54cec74372b4fea1aeed4))
- **feat**: Add glob pattern support for copy operations - ([f5887bb](https://github.com/h-michael/kabu/commit/f5887bbb3eadbdc9bcd65b9adc664e6b1c862ce6))
- **feat**: Add readline keybindings for text input - ([b0bc936](https://github.com/h-michael/kabu/commit/b0bc9360e609b45d30f7904f853ece88ad383d16))
- **refactor**: Rewrite docs, help text, and examples - ([bbbd385](https://github.com/h-michael/kabu/commit/bbbd38569f0d2b0d036e005654a0b1c2b7980b38))
- **chore**(ci): Bump orhun/git-cliff-action from 4.7.0 to 4.7.1 - ([833864a](https://github.com/h-michael/kabu/commit/833864a23dbd0129f7cd7761e498fc700232de35))
- **chore**(ci): Bump taiki-e/upload-rust-binary-action from 1.28.0 to 1.28.1 - ([7ccc049](https://github.com/h-michael/kabu/commit/7ccc049a1b0962da1170556be01344c931dadf8b))
- **chore**(ci): Bump actions/cache from 5.0.3 to 5.0.4 - ([2f417b6](https://github.com/h-michael/kabu/commit/2f417b6079f67438f4a556dc28c99ca5668f7f82))
- **chore**(ci): Bump docker/setup-buildx-action from 3.12.0 to 4.0.0 - ([e813f6f](https://github.com/h-michael/kabu/commit/e813f6f5d36f812188cf7cebad7d3323e6587433))
- **chore**(ci): Bump softprops/action-gh-release from 2.5.0 to 2.6.1 - ([ada66bc](https://github.com/h-michael/kabu/commit/ada66bcec08f08a4cdc298649eb2bbaff2545e8e))
- **chore**(ci): Bump taiki-e/upload-rust-binary-action from 1.28.1 to 1.29.1 - ([f896c76](https://github.com/h-michael/kabu/commit/f896c769e8175ebd6f5aa58902264959b0081ebc))
- **fix**: Skip tracked directories in glob link expansion with skip_tracked - ([5267500](https://github.com/h-michael/kabu/commit/526750013d71664ad36910aa5aef4cbd035bc4f2))
- **fix**: Adapt to clap_mangen 0.3 and sha2 0.11 breaking changes - ([8cde812](https://github.com/h-michael/kabu/commit/8cde8127f2cba4b7889c35396ba61e62eeac22c5))
- **fix**: Delete newly created branch on setup rollback - ([f433a1c](https://github.com/h-michael/kabu/commit/f433a1c725fe9dfc41874dc00adecea790d7591d))
- **feat**: Add on_setup_failure config for rollback behavior - ([079873b](https://github.com/h-michael/kabu/commit/079873b121744d7c0e843c874bcd07f38981fdaf))
- **docs**: Update schema and CLI help for on_setup_failure config - ([773a2f8](https://github.com/h-michael/kabu/commit/773a2f8da218af754ed16bc922807b98e6aa23ca))
- **fix**: Add on_setup_failure field to test config in trust.rs - ([b30ea69](https://github.com/h-michael/kabu/commit/b30ea69daa9c787e87ab770baadeac5abd0e79f7))
## [0.5.0](https://github.com/h-michael/kabu/compare/v0.4.0..v0.5.0) - 2026-01-14


- **feat**: Add trusted publishing support for crates.io - ([22fc2d4](https://github.com/h-michael/kabu/commit/22fc2d439daa70a9e220a9956fa0ea39c8565f39))
- **feat**: [**breaking**] Migrate configuration from TOML to YAML with JSON Schema - ([c2501c5](https://github.com/h-michael/kabu/commit/c2501c57bde59fe1bb0dffd15ec7ff5abaea5ea0))
- **fix**: Filter out symbolic refs from remote branch list - ([1c6000a](https://github.com/h-michael/kabu/commit/1c6000aca2277357fe75e7b4c0ff4443be0e9c61))
- **fix**: Reject unknown fields in YAML configuration - ([0f67c64](https://github.com/h-michael/kabu/commit/0f67c64afa29caf0a8b9ce4e349bf7f4f05deaf2))
- **feat**: [**breaking**] Add branch_template support for interactive branch creation - ([4d0ca0e](https://github.com/h-michael/kabu/commit/4d0ca0edcddf5e42189e5c00b9be2e5dd6c3f10c))
- **feat**: [**breaking**] Require double braces for template variables - ([977fcf4](https://github.com/h-michael/kabu/commit/977fcf498e042d5090f20aca5a17d4a937431674))
- **feat**: Add clippy lints configuration - ([d91de6f](https://github.com/h-michael/kabu/commit/d91de6f235decb04f7ee9279669edc439f498614))
- **fix**: Clear screen immediately when entering interactive mode - ([0bd6cae](https://github.com/h-michael/kabu/commit/0bd6caebf771bc235cfb2d081daf6d57a162218a))
- **refactor**: Move color options from global to per-command scope - ([5c9814f](https://github.com/h-michael/kabu/commit/5c9814fb5f7b7ddb7ffa4c615e8245af7e2dcb7f))
## [0.4.0](https://github.com/h-michael/kabu/compare/v0.3.0..v0.4.0) - 2026-01-13


- **feat**: Add hooks with trust mechanism - ([8f77ecf](https://github.com/h-michael/kabu/commit/8f77ecfacc8ebf0c12c0ade4d9c603d1874319da))
- **feat**: Add gwtx list command - ([de2f16d](https://github.com/h-michael/kabu/commit/de2f16d3c6c8ec5f5d05b7859d799640b2f1e31d))
- **feat**: Add hook description field and color output options - ([4a222db](https://github.com/h-michael/kabu/commit/4a222dbe83eaf30b323e178f1f82677b4839089d))
- **refactor**: Improve code quality and documentation - ([48e55bb](https://github.com/h-michael/kabu/commit/48e55bbb47ac96c841043b066b630ed30d0d4466))
- **feat**: Integrate skim fuzzy finder for Unix platforms - ([0178cae](https://github.com/h-michael/kabu/commit/0178cae33d95e8f675a41a64b717d3b9f5552809))
- **fix**: Enable directory symlinks with glob patterns - ([fcab10c](https://github.com/h-michael/kabu/commit/fcab10cc2abe578fd74d40705d5745061ee7f162))
- **feat**: Add worktree path configuration with variable expansion - ([425c5ca](https://github.com/h-michael/kabu/commit/425c5caf9074061ef4d3a1644a8b5a2701e47ea4))
- **feat**: Add color output to remove command warnings - ([5eb286b](https://github.com/h-michael/kabu/commit/5eb286baf2a70a3621b9521f04bdf0180dcccf08))
- **docs**: Organize examples into dedicated directory - ([a57e780](https://github.com/h-michael/kabu/commit/a57e780e7bd703304a767183f1b29d493f686a96))
## [0.3.0](https://github.com/h-michael/kabu/compare/v0.2.0..v0.3.0) - 2026-01-11


- **chore**(deps): Add Dependabot configuration - ([672b7a8](https://github.com/h-michael/kabu/commit/672b7a8e74e8f16c73369e05aebc9fb16c7d09bf))
- **docs**: Add contributing guide - ([af4804e](https://github.com/h-michael/kabu/commit/af4804efb89fe09118ab2520b1b9a87d6d72ef08))
- **feat**: Add remove command - ([16fcf61](https://github.com/h-michael/kabu/commit/16fcf610de135fad908301d756a362b0808ffadd))
- **docs**: Add missing git worktree options to README - ([99ba301](https://github.com/h-michael/kabu/commit/99ba301d03af3bcf9a2697a6cdd610b28e7178e4))
- **chore**: Update git-cliff configuration - ([df8c7fb](https://github.com/h-michael/kabu/commit/df8c7fb78ae37b39220e02fb78bfb78977f4472d))
- **chore**(ci): Bump actions/checkout from 4.3.1 to 6.0.1 - ([0da41e2](https://github.com/h-michael/kabu/commit/0da41e2d011c696dcf19e745759ff9dd4fcde064))

## New Contributors

* @dependabot[bot] made their first contribution

## [0.2.0](https://github.com/h-michael/kabu/compare/v0.1.2..v0.2.0) - 2026-01-11


- **docs**: Add INSTALL.md and flake.nix - ([d61ab5a](https://github.com/h-michael/kabu/commit/d61ab5a20b974d6984bf173c2fc327b0c5f24a10))
- **docs**: Add badges to README - ([d6059dd](https://github.com/h-michael/kabu/commit/d6059dd005245725162c4b70f196ca6c03413471))
- **feat**: Add glob pattern support with skip_tracked option - ([bd75501](https://github.com/h-michael/kabu/commit/bd755011d7209b1d251a51ae5e1e08809a154d7e))
- **docs**: Improve config examples with realistic use cases - ([0a1676e](https://github.com/h-michael/kabu/commit/0a1676e560d187fee6733396959bef26e91d8537))
- **chore**: Bump version to 0.2.0 - ([3e7e1a4](https://github.com/h-michael/kabu/commit/3e7e1a477ccfa41a0d3f1c0fb75a88d769782853))
## [0.1.2](https://github.com/h-michael/kabu/compare/v0.1.1..v0.1.2) - 2026-01-05


- **chore**: Bump version to 0.1.2 - ([1af2b0d](https://github.com/h-michael/kabu/commit/1af2b0d1c47a7fc984e1005e2b7a1af4f3130f54))
## [0.1.1] - 2026-01-05


- **refactor**: Make config file optional - ([9a51929](https://github.com/h-michael/kabu/commit/9a5192909fb4ee6fd137e7d77a8e4ddaefba5ecf))
- **fix**: Detect Unix-style absolute paths on Windows - ([0bce1cd](https://github.com/h-michael/kabu/commit/0bce1cddd7eb81c1cb5d26f78cc6ba097e747c31))
- **fix**: Add write permission for release workflow - ([fd91b72](https://github.com/h-michael/kabu/commit/fd91b7296f7164b6f7799bd2a4db3a1ef0d82171))
- **chore**: Bump version to 0.1.1 and use draft releases - ([93762c5](https://github.com/h-michael/kabu/commit/93762c5ae7da689cdec03547f0d69dad9fb444b4))

## New Contributors

* @h-michael made their first contribution in [#5](https://github.com/h-michael/kabu/pull/5)

<!-- generated by git-cliff -->
