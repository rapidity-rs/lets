# Changelog

All notable changes to this project will be documented in this file.
See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## [v0.4.0](https://github.com/rapidity-rs/lets/compare/125a37e33523203e1091b1280cd6b9dd1aa66944..v0.4.0) - 2026-07-20
#### Features
- (**cli**) JSON listing, bare-invocation picker, and min-version gate - ([82b9f14](https://github.com/rapidity-rs/lets/commit/82b9f14ffbb7b11d7b8edf1d82f9c03a749f20ce)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**cli**) variadic, typed, optional args plus flag choices and env fallbacks - ([b34e818](https://github.com/rapidity-rs/lets/commit/b34e81880611526c947c2ab2eb28aab7d6a89611)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) command echo, --keep-going, --summary, and CI fold markers - ([0243bff](https://github.com/rapidity-rs/lets/commit/0243bff529743da0660c361eefaa59e35d5c3c40)) - [@taylorFaucett](https://github.com/taylorFaucett)
- ![BREAKING](https://img.shields.io/badge/BREAKING-red) (**exec**) run commands from the config-file directory - ([3852bfe](https://github.com/rapidity-rs/lets/commit/3852bfe99b1d724017e7832959d98eb98038ecf0)) - [@taylorFaucett](https://github.com/taylorFaucett)
- ![BREAKING](https://img.shields.io/badge/BREAKING-red) (**interpolate**) strict placeholders, brace escapes, hook interpolation - ([125a37e](https://github.com/rapidity-rs/lets/commit/125a37e33523203e1091b1280cd6b9dd1aa66944)) - [@taylorFaucett](https://github.com/taylorFaucett)
- ![BREAKING](https://img.shields.io/badge/BREAKING-red) (**parse**) reject typos, duplicate nodes, and colliding names at load - ([f163cb5](https://github.com/rapidity-rs/lets/commit/f163cb557baf9b77e52a922641aa9fddb5ba2499)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**vars**) dynamic cmd= vars and project-wide config env - ([9d05819](https://github.com/rapidity-rs/lets/commit/9d058198b4dec1b1859b3a578796e980227e8861)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Documentation
- upgrading guide, escaping example, Windows status, link fixes - ([cd32e66](https://github.com/rapidity-rs/lets/commit/cd32e66fe349dbfb989d0ed8e099b9d30b614b62)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Miscellaneous Chores
- (**release**) commit Cargo.lock and build releases --locked - ([2759aaa](https://github.com/rapidity-rs/lets/commit/2759aaa0809b0b1661339f4bbfae97716e540874)) - [@taylorFaucett](https://github.com/taylorFaucett)

- - -

## [v0.3.0](https://github.com/rapidity-rs/lets/compare/7a84364575ca36cebe6b4e9ff40fcdd2f345a52e..v0.3.0) - 2026-07-19
#### Features
- (**exec**) run-policy always opts a task out of memoization (#25) - ([d3ddd86](https://github.com/rapidity-rs/lets/commit/d3ddd86deaec43fd03cbd97d48bf9f83983c0de5)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) cap task concurrency with config jobs and --jobs (#24) - ([593609b](https://github.com/rapidity-rs/lets/commit/593609b0bd26d2e8df1b8a198ca160641e393092)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**fingerprint**) skip up-to-date tasks by checksumming sources (#23) - ([857a485](https://github.com/rapidity-rs/lets/commit/857a485610cd21b409f38ec59e0432cbde703f03)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**vars**) global and scoped variables in interpolation (#26) - ([e50b792](https://github.com/rapidity-rs/lets/commit/e50b792192ba88091fea596ba4075bbb61733c59)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Miscellaneous Chores
- (**deps**) bump actions/upload-pages-artifact from 4 to 5 (#31) - ([062efa0](https://github.com/rapidity-rs/lets/commit/062efa0326380a54225a0afb09134e5fc1d27149)) - dependabot[bot], dependabot[bot]
- (**deps**) bump actions/checkout from 4 to 7 (#30) - ([ed43c7d](https://github.com/rapidity-rs/lets/commit/ed43c7df45e7fa5be8a52afab045cd801a06ff73)) - dependabot[bot], dependabot[bot]
- (**deps**) bump actions/setup-node from 4 to 7 (#29) - ([149b510](https://github.com/rapidity-rs/lets/commit/149b510fa11cbb23178d6045b99d79d0d33f3b6e)) - dependabot[bot], dependabot[bot]
- (**deps**) bump amannn/action-semantic-pull-request from 5 to 6 (#28) - ([beabcc6](https://github.com/rapidity-rs/lets/commit/beabcc6cc08317f5367dde4238137d71309a5914)) - dependabot[bot], dependabot[bot]
- (**deps**) bump softprops/action-gh-release from 2 to 3 (#27) - ([7f7daa5](https://github.com/rapidity-rs/lets/commit/7f7daa5708281d6b770dc09425d0fa3218457b92)) - dependabot[bot], dependabot[bot]
- (**deps**) bump actions/deploy-pages from 4 to 5 (#5) - ([a55f021](https://github.com/rapidity-rs/lets/commit/a55f021e1d6b64cf0cdda36f471580b4e7ad4f79)) - dependabot[bot], dependabot[bot]
- (**deps**) bump actions/download-artifact from 4 to 8 (#2) - ([efb6f04](https://github.com/rapidity-rs/lets/commit/efb6f0422d52900b581cccfda20041e4920961c2)) - dependabot[bot], dependabot[bot]
- (**deps**) bump actions/configure-pages from 5 to 6 (#3) - ([7a84364](https://github.com/rapidity-rs/lets/commit/7a84364575ca36cebe6b4e9ff40fcdd2f345a52e)) - dependabot[bot], dependabot[bot]
- (**release**) dispatch Homebrew tap bump after cog bump (#32) - ([870615d](https://github.com/rapidity-rs/lets/commit/870615d0eff9700e14fd11cfbca146fd0a9a219e)) - [@taylorFaucett](https://github.com/taylorFaucett)

- - -

## [v0.2.0](https://github.com/rapidity-rs/lets/compare/59dc34dd84ce27015000c9bee51506c9b48713a9..v0.2.0) - 2026-07-19
#### Features
- (**docs**) rebuild documentation site with Astro Starlight - ([856e2af](https://github.com/rapidity-rs/lets/commit/856e2af5b8c7e65929819d4b24ab5d06671ad4ef)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**examples**) runnable example gallery wired into tests and docs (#17) - ([0e72dbf](https://github.com/rapidity-rs/lets/commit/0e72dbfe7b924bf8069b75ad688ea9a9f1d27f40)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) defer hooks with graceful interrupt handling (#19) - ([6ad0a7e](https://github.com/rapidity-rs/lets/commit/6ad0a7e2a945a839326fc8b3b30d73dc731947f8)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) precondition and status gates for tasks (#18) - ([3469a26](https://github.com/rapidity-rs/lets/commit/3469a26c28cccc24da56b4c59e9c4807fd8e38d6)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) run each task at most once per invocation - ([0841c04](https://github.com/rapidity-rs/lets/commit/0841c04706209c8f3f3185ad2d383a5fb2969d42)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**orch**) support arguments in deps and steps references - ([d2573da](https://github.com/rapidity-rs/lets/commit/d2573daad5e5e534d6ec6ed00e2f9a7eae16c0ce)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**output**) interleaved, group, and prefixed output modes - ([45fc9e9](https://github.com/rapidity-rs/lets/commit/45fc9e99aee77beaba1fe6e3980b0d5282d55190)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**watch**) re-run commands on source changes with --watch - ([76f57f9](https://github.com/rapidity-rs/lets/commit/76f57f906bbe8ec851eb06ff222c74daaf3a9897)) - [@taylorFaucett](https://github.com/taylorFaucett)
- add mise support - ([57aaa3f](https://github.com/rapidity-rs/lets/commit/57aaa3ffb8fa2c0f8a400e726607ab9eb292d5f2)) - [@taylorFaucett](https://github.com/taylorFaucett)
- add dependabot with a weekly cycle - ([75b2183](https://github.com/rapidity-rs/lets/commit/75b218354e1e881970b90393c98d4d91b50b1bb1)) - [@taylorFaucett](https://github.com/taylorFaucett)
- show help with self commands when no lets.kdl is present - ([c651172](https://github.com/rapidity-rs/lets/commit/c651172d1ab479573a9a69ac5e6d46ca8683671c)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Bug Fixes
- (**exec**) canonicalize task keys from parsed arguments (#21) - ([a3a9a31](https://github.com/rapidity-rs/lets/commit/a3a9a318a9b6871fa0132170665e9ec94469385d)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**exec**) collect prompts and confirms across the whole task graph - ([59939b4](https://github.com/rapidity-rs/lets/commit/59939b48326411d6259166538b5d04b3429d8384)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**interactive**) require explicit default for choose under --yes - ([87c3af5](https://github.com/rapidity-rs/lets/commit/87c3af5a58091e3b6087fb2db3f16ee96df67f51)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**watch**) restart when included config files change (#20) - ([d26f2c9](https://github.com/rapidity-rs/lets/commit/d26f2c934b4f1ab17fa3fb89161aca203e59b0b1)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**watch**) react only to file mutations, not reads (#6) - ([a818246](https://github.com/rapidity-rs/lets/commit/a818246e891098c01c449ad92a9f0f0593c77989)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Documentation
- (**readme**) document output modes - ([bc65dae](https://github.com/rapidity-rs/lets/commit/bc65daec1401e3bb3a90c0778bcda88ca70ad58d)) - [@taylorFaucett](https://github.com/taylorFaucett)
- document sources and watch mode - ([8e22d1f](https://github.com/rapidity-rs/lets/commit/8e22d1f986b0b883efc7fc7ef21ccbaa53c175dc)) - [@taylorFaucett](https://github.com/taylorFaucett)
- document run-once tasks, ref arguments, and confirm guards - ([16a327d](https://github.com/rapidity-rs/lets/commit/16a327d3595a161233271b98766157454387deb0)) - [@taylorFaucett](https://github.com/taylorFaucett)
- add MIT license and changelog - ([1f9ece9](https://github.com/rapidity-rs/lets/commit/1f9ece9fec48f6fca15b53bb48199bc52a9b2a09)) - [@taylorFaucett](https://github.com/taylorFaucett), Claude Opus 4.6 (1M context)
#### Continuous Integration
- enforce conventional PR titles and commit messages - ([6e2cbcb](https://github.com/rapidity-rs/lets/commit/6e2cbcbd0e302c960724ca0b4f276acf685b458d)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Refactoring
- (**tree**) introduce TaskRef for deps/steps references - ([704ec53](https://github.com/rapidity-rs/lets/commit/704ec53132b7a4c9943499504f3817a14809832e)) - [@taylorFaucett](https://github.com/taylorFaucett)
#### Miscellaneous Chores
- (**changelog**) use cocogitto separator - ([6d88d49](https://github.com/rapidity-rs/lets/commit/6d88d4948ddc3b80493075b43f0d009ce06c436f)) - [@taylorFaucett](https://github.com/taylorFaucett)
- (**deps**) bump actions/upload-artifact from 4 to 7 (#4) - ([6c65bc5](https://github.com/rapidity-rs/lets/commit/6c65bc5b30e33aeb438157e56c0c9a10b7e2dd9a)) - dependabot[bot], dependabot[bot]
- (**docs**) remove zensical documentation site - ([4e3e4d2](https://github.com/rapidity-rs/lets/commit/4e3e4d26c3b17d769880779df5dffae027e1368f)) - [@taylorFaucett](https://github.com/taylorFaucett)
- pin npm and cocogitto via mise - ([ce5cc0a](https://github.com/rapidity-rs/lets/commit/ce5cc0a0116eff97899c19bcd0e809be11028160)) - [@taylorFaucett](https://github.com/taylorFaucett)
- add cocogitto and pre-commit configuration - ([59dc34d](https://github.com/rapidity-rs/lets/commit/59dc34dd84ce27015000c9bee51506c9b48713a9)) - [@taylorFaucett](https://github.com/taylorFaucett), Claude Opus 4.6 (1M context)

- - -


## [v0.1.0](https://github.com/rapidity-rs/lets/compare/v0.1.0..v0.1.0) - 2026-04-04
#### Features
- initial implementation - ([7764421](https://github.com/rapidity-rs/lets/commit/7764421456b3ced2250d99d0f96f35e000955068)) - [@taylorFaucett](https://github.com/taylorFaucett)
