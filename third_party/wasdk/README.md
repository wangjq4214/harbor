# Windows App SDK binding inputs

`gen/` regenerates the checked-in `src/backdrop/wasdk/generated.rs` from pinned Microsoft Windows App SDK NuGet metadata. These packages are build-time reference material only, not a bundled runtime. The normal Cargo build uses the checked-in bindings and does not need the packages.

Run `./scripts/fetch_wasdk.ps1` on Windows before regenerating. It restores only:

| Package | Version | SHA256 of NuGet package |
| --- | --- | --- |
| `Microsoft.WindowsAppSDK.InteractiveExperiences` | `1.8.260708001` | `496EEA92D353B5D3601B67353F06DCADD6D2D9B635575ACEBE6E42587DBFAD76` |
| `Microsoft.WindowsAppSDK.Foundation` | `1.8.260803002` | `B9232041AFD605B606C6F78F442D92EAD0076453F1F2A3260D2B7F8089BCAB0E` |

`-PackageDirectory <path>` accepts predownloaded `interactive.nupkg` and `foundation.nupkg` for offline use; package hashes and required metadata are checked before publishing SDK inputs. Downloads are cached in ignored `target/wasdk-download/`. The extracted `interactive/metadata/`, `foundation/metadata/` and `interactive/include/Microsoft.UI.Interop.h` remain ignored local reference material; do not commit package payloads. Update package version, hash, `pinned-version.txt`, generator metadata assumptions and checked-in bindings together when upgrading.

To regenerate from the repository root (after fetching):

```powershell
cd third_party/wasdk/gen
cargo run
```

The generated file is reviewed and committed separately; fetching does not modify it.
