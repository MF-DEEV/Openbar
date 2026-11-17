# Windows memory details

Open **System > Memory**. The **Memory details** panel is below the existing RAM
reading and chart, above the process list. Hover over a detail for its definition.
Run the native app (`npm run tauri dev` from `windows`) to see live measurements;
the browser preview has no native source and shows **Unavailable** for these rows.

| Row | Source and meaning |
| --- | --- |
| Memory pressure (approx.) | `K32GetPerformanceInfo`: the higher of `1 - PhysicalAvailable / PhysicalTotal` and `CommitTotal / CommitLimit`, displayed as a percentage. This is a Windows headroom approximation, not the macOS pressure algorithm or a prediction of paging activity. |
| Compressed memory | `NtQuerySystemInformation(SystemProcessInformation)`: `WorkingSetSize` of the kernel's `MemCompression` / `Memory Compression` process, with parent PID 4. This measures resident store RAM, not the original uncompressed page size, bytes saved, or a process's private commit. If the store is hidden, absent, or cannot be read, the value is unavailable, not zero. |
| Cached memory | `K32GetPerformanceInfo`: `SystemCache * PageSize`. Windows defines this as standby memory plus the system working set. It differs from macOS Cached Files and Task Manager's Cached (standby plus modified pages). |
| Page file usage | `K32EnumPageFilesW`: sum of `TotalInUse * PageSize` across installed page files. This is current pagefile usage, not commit charge, capacity, cumulative writes, or a measurement of the separate Windows `swapfile.sys`. A successful enumeration with no pagefiles means zero; failure means unavailable. |

New measurements are taken only while the Memory detail is open, at the existing
monitor interval. No elevated permissions or new dependencies are required.
The original RAM readings, graph history, and private working set process ranking
use their existing sources and formulas.

API references:

- [PERFORMANCE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information)
- [ENUM_PAGE_FILE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-enum_page_file_information)
- [EnumPageFilesW](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-enumpagefilesw)

Validation:

```powershell
# From windows
npm run typecheck
npm test
npm run build
npx prettier --check src/system/MetricDetail.tsx src/system/types.ts src/system/strings.ts src/system/detail.ts src/system/detail.test.ts src/system/demo.ts docs/memory-details.md

# From windows/src-tauri
cargo fmt --check
cargo check --offline
cargo test --offline system_monitor --lib
cargo test --offline live_memory_details --lib -- --ignored --nocapture
```

The last command reads the host's actual memory APIs and checks snapshot delivery
and clearing when switching to CPU. Compression may legitimately be unavailable.
For a manual comparison, use Task Manager's **Performance > Memory** for physical
RAM, and `Get-CimInstance Win32_PageFileUsage` (`CurrentUsage`, in MiB) for pagefiles.
Do not compare pagefile usage to Task Manager's **Committed** value.
