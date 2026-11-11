# SCREENSHOT-MANIFEST

Corpus: `easeus-drw-research/re-workspace/raw-evidence/ui-screenshots/` — 63 captures.
All captures: light theme, ~1024×735 client on a 1024×768 runner desktop (100% DPI),
comfortable density, maximized frameless window. Structured VLM extraction for every
one of the 63 captures lives in `docs/evidence/vlm/shots/` (per-shot JSON: layout
regions, verbatim text, hex candidates marked ESTIMATED, typography, control states,
state classification — cross-checked against the QSS census and string tables, where
hard evidence wins); curated state labels below derive from the corpus
README reading pass. Commercial/monetization surfaces (retention/license gates) are
captured for completeness of the corpus but are OUT OF SCOPE for this re-implementation
(documented divergence — see UI-MAP §0).

## 01 Splash

| File                         | VLM page                      | State                      | Annotation (VLM note)                                                                                          |
| ---------------------------- | ----------------------------- | -------------------------- | -------------------------------------------------------------------------------------------------------------- |
| splash-reading-disk-info.png | Splash Screen / Loading State | loading (disk enumeration) | Modern splash screen utilizing 3D isometric art to visualize abstract data recovery concepts during a block... |
| splash-t35.png               | Splash Screen / Loading State | loading (extended)         | Modern 'glassmorphism' or soft-UI aesthetic with 3D isometric illustration elements.                           |

## 02 Landing / Home

| File                                     | VLM page                                        | State                         | Annotation (VLM note)                                                                                          |
| ---------------------------------------- | ----------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------- |
| landing-activate-input-focus.png         | Activation Dialog                               | license dialog, input focused | The UI uses a modern, clean aesthetic with a prominent activation overlay blocking the primary workflow unt... |
| landing-after-activate-close.png         | Select Location (Step 1)                        | post-close restore            | Modern Fluent Design-inspired UI with soft shadows, rounded corners, and a clear visual hierarchy for the p... |
| landing-ai-assistant-panel.png           | Select Location / Main Wizard                   | panel open                    | The UI uses a friendly, approachable aesthetic with soft blues and rounded corners to mitigate the stress o... |
| landing-ai-opt1-solution-wizard.png      | Select Location (Step 1/3)                      | wizard open                   | The UI features a prominent contextual assistant (mascot-based) that overlays the main content to guide the... |
| landing-ai-opt1-solution-wizard-1of3.png | Select Location (Step 1) with Assistant Overlay | wizard step 1/3               | The UI features an integrated AI assistant that provides contextual troubleshooting overlays during the sel... |
| landing-baseline.png                     | Select Location / Main Wizard                   | success / baseline            | The UI uses a friendly, wizard-driven approach with large touch targets and clear visual hierarchy for driv... |
| landing-baseline-v2.png                  | Select Location (Hardware Disk)                 | success / baseline (run 2)    | The UI uses a clean, modern 'fluent' style with soft backgrounds and rounded corners, prioritizing large to... |
| landing-buy-activate-dialog.png          | Activation Dialog (Modal Overlay)               | license dialog                | The UI uses a standard Windows 11 aesthetic with Fluent Design influences, featuring soft shadows and round... |
| landing-desktop-tile-clicked.png         | Select Location / Hardware Disk                 | selected tile                 | Modern Windows 11-style UI with rounded corners, soft shadows, and a clear visual hierarchy emphasizing the... |
| landing-early.png                        | Select Location / Hardware Disk                 | loading (device enumeration)  | Modern Windows 11-style UI using Fluent Design principles with soft shadows, rounded corners, and a clean b... |
| landing-hamburger-menu-open.png          | Select Location (Main Wizard)                   | menu open (13 items)          | The UI utilizes a clean, modern 'Fluent-like' design language with soft blue accents to guide the user's ey... |
| landing-sidebar-chronosnap-page.png      | ChronoSnap Backup Feature Landing               | sidebar page                  | The UI uses a clean, modern 'SaaS-style' aesthetic within a desktop application framework, featuring a prom... |
| landing-sidebar-filerepair-page.png      | File Repair Selection Screen                    | sidebar page                  | The UI utilizes a friendly mascot (robot with puzzle piece) to soften the technical nature of data repair.     |
| landing-sidebar-lostpartition-page.png   | Lost Partition Rescue                           | sidebar page                  | The UI uses a clean, modern 'SaaS-style' aesthetic within a desktop application framework, featuring a masc... |
| landing-sidebar-moretools-confirm.png    | More Recovery Tools                             | sidebar page (confirm run)    | Clean, modern Windows 11-style interface using a card-based layout for feature discovery.                      |
| landing-sidebar-moretools-page.png       | More Recovery Tools                             | sidebar page (tool grid)      | The UI uses a clean, modern 'card-based' layout typical of contemporary Windows applications, emphasizing d... |
| landing-sidebar-sdcard-page.png          | SD Card Recovery - Connection Screen            | sidebar page                  | The UI uses a friendly, illustrative style with a mascot to guide the user through the hardware connection ... |
| landing-solution-wizard-1of3.png         | Select Location / Solution Dialog               | wizard step 1/3               | The UI features a 'Solution' overlay that appears to be an interactive troubleshooting guide or tutorial step. |
| landing-solution-wizard-2of3.png         | Select Location / Solution Dialog               | wizard step 2/3               | The UI uses a modern, clean aesthetic with a prominent blue accent color for primary actions and a dark hea... |
| landing-solution-wizard-3of3.png         | Select Location / Solution Dialog               | wizard step 3/3               | The application uses a modern Fluent Design-inspired UI with a prominent blue accent color and rounded corn... |
| landing-solution-wizard-closed.png       | Select Location / Device Detection              | post-close restore            | The UI uses a modal overlay approach to guide the user through device detection while keeping the backgroun... |

## 03 Scan Phase

| File                         | VLM page                                 | State                     | Annotation (VLM note)                                                                                          |
| ---------------------------- | ---------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------- |
| scan-after-stop-attempt.png  | Stop Searching Dialog                    | confirm dialog persistent | The UI uses a standard Windows 11 aesthetic with rounded corners on the modal and buttons, and a clear visu... |
| scan-midscan-210s.png        | Scan Results / File Explorer             | scanning deep phase       | Classic Windows utility aesthetic using a clean light theme with high-information density for technical dat... |
| scan-midscan-21pct.png       | Scan Results / File Explorer View        | scanning 21% (gauge)      | Classic Windows utility aesthetic using a blue/orange accent palette with a persistent scanning status bar.    |
| scan-midscan-34pct-prior.png | File List / Scan Results                 | scanning 34%, live tree   | The UI follows a standard Windows 11/Fluent Design aesthetic with a focus on information density during act... |
| scan-paused.png              | Scan Results / File Explorer View        | paused                    | Classic Windows utility aesthetic updated for Win 11 with rounded corners and Mica-like transparency effects.  |
| scan-resumed.png             | Scan Results / File Browser              | resumed                   | Classic utility software layout optimized for high information density during long-running background tasks.   |
| scan-stop-confirm-dialog.png | Scan Progress / Stop Confirmation Dialog | confirm dialog            | The UI follows a modern Windows 11 aesthetic with Fluent Design influences, featuring a prominent modal tha... |

## 04 Scan Complete

| File                              | VLM page             | State                  | Annotation (VLM note)                                                                                          |
| --------------------------------- | -------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| complete-celebration-modal.png    | Scan Complete Dialog | complete modal (stats) | The UI uses a celebratory modal to transition the user from a passive scanning state to an active recovery ... |
| complete-celebration-modal-v2.png | Scan Complete Dialog | complete modal (run 2) | The UI uses a clear visual hierarchy to celebrate the successful completion of a long-running background ta... |

## 05 Results

| File                                    | VLM page                           | State                      | Annotation (VLM note)                                                                                          |
| --------------------------------------- | ---------------------------------- | -------------------------- | -------------------------------------------------------------------------------------------------------------- |
| results-baseline.png                    | Scan Results / File Browser        | success / baseline         | The interface uses a clean, modern Windows 11 aesthetic with a prominent AI integration point for recommend... |
| results-baseline-v2.png                 | Scan Results / File Explorer View  | success / baseline (run 2) | Modern Windows 11-style UI with Fluent Design influences, featuring a prominent AI integration point via a ... |
| results-context-menu-open.png           | Scan Results / File Browser        | context menu open          | Standard Windows 11-style Fluent Design application with a focus on high-information density for data recov... |
| results-context-menu-open-v2.png        | Scan Results / File Explorer View  | context menu open (v2)     | Modern Windows 11-style UI with rounded corners, Mica-like transparency effects, and a prominent AI-assiste... |
| results-context-menu-round4.png         | Scan Results / File Explorer       | context menu open (v4)     | The UI follows a modern Fluent/Material hybrid style with a prominent AI integration point via a floating a... |
| results-filter-bar-open.png             | Scan Results / File Explorer View  | filter bar open            | Modern Windows 11-style UI featuring a prominent AI recommendation overlay and a high-density data table fo... |
| results-filter-bar-open-clean.png       | Scan Results / File Explorer       | filter bar open (clean)    | Classic Windows utility aesthetic with modern flat-design influences and a prominent AI feature toggle.        |
| results-filter-path-dropdown.png        | Scan Results / File Browser        | filter path dropdown open  | Classic Windows utility aesthetic using a clean white/blue palette with a prominent 'Recover All' call-to-a... |
| results-path-tab.png                    | File Type Selection / Scan Results | path tree tab              | Classic Windows utility aesthetic with a focus on high-information density while maintaining readability th... |
| results-row-selected.png                | File Type Selection / Scan Results | row selected               | Modern Windows 11-style UI with a focus on visual file type identification through large icons.                |
| results-row-selected-gate-dismissed.png | Scan Results / File Browser        | row selected post-dismiss  | Classic Windows utility aesthetic using a clean blue accent palette and a three-pane explorer-style layout.    |
| results-row-selected-round4.png         | Scan Results / File Browser        | row selected (run 4)       | Modern Fluent Design-inspired interface with high information density balanced by clear visual hierarchy an... |
| results-search-active.png               | Scan Results / File Explorer       | search active + chip       | The UI follows modern Windows 11 design patterns with rounded corners, fluent-style icons, and a clean whit... |
| results-type-tab-midtransition.png      | Scan Results / File Explorer View  | type tab (mid-transition)  | Classic Windows utility aesthetic using a clean, high-information-density layout with a distinct three-pane... |

## 06 Preview

| File                           | VLM page                 | State                    | Annotation (VLM note)                                                                                          |
| ------------------------------ | ------------------------ | ------------------------ | -------------------------------------------------------------------------------------------------------------- |
| preview-panel-html-content.png | Scan Results / File List | preview pane (text/html) | Classic Windows utility aesthetic updated with modern rounded corners and a soft blue color palette.           |
| preview-panel-live.png         | File List / Scan Results | preview pane (image)     | Standard Windows 11 Fluent Design influence with a focus on visual scanning via large thumbnails and clear ... |

## 07 Recovery

| File                                  | VLM page                                          | State                         | Annotation (VLM note)                                                                                          |
| ------------------------------------- | ------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------- |
| recovery-after-esc.png                | Scan Results / File Explorer                      | gate post-ESC                 | Standard Windows 11 Fluent Design influence with rounded corners and soft shadows.                             |
| recovery-destdlg-choosefolder.png     | Save Destination Dialog                           | folder picker                 | The UI uses a standard Windows 11 aesthetic with rounded corners and Fluent Design influences, specifically... |
| recovery-destdlg-cloud-tab.png        | Cloud Save Destination Dialog                     | cloud tab                     | The UI uses a clean, modern Windows 11 aesthetic with soft shadows and a clear visual hierarchy for the mul... |
| recovery-destdlg-early-state.png      | Save Destination Dialog                           | destination dialog (early)    | Standard Windows 11 Fluent Design influence with clear visual hierarchy and safety warnings for data recovery. |
| recovery-destdlg-local-selected.png   | Destination Selection Dialog                      | destination row selected      | The UI uses a clear visual hierarchy to warn the user against saving recovered data to the source drive (C:).  |
| recovery-destdlg-repair-off.png       | Save Destination Dialog                           | repair toggle off             | The UI uses standard Windows 11/Fluent design patterns with clear visual feedback for selection and system ... |
| recovery-destination-dialog.png       | Destination Selection Dialog                      | destination dialog            | The UI uses color-coded warning icons to indicate insufficient space or recovery risks on specific drives.     |
| recovery-destination-dialog-clean.png | Save File Destination Dialog                      | destination dialog (clean)    | The UI follows modern Fluent/Material design principles with soft shadows, rounded corners, and clear visua... |
| recovery-entry-results.png            | Scan Results / File Explorer View                 | entry from results            | The UI utilizes a clean, modern 'Fluent' design style with soft shadows and rounded corners, featuring an i... |
| recovery-exit-retention-gate.png      | Data Recovery Results - Risk Warning Dialog       | exit gate                     | The UI uses an aggressive modal overlay to interrupt the user workflow for upselling purposes, utilizing hi... |
| recovery-insufficient-space-gate.png  | Recovery Results / Insufficient Disk Space Dialog | error / space gate            | The UI uses a standard Windows 11-style aesthetic with rounded corners and a soft color palette, featuring ... |
| recovery-retention-gate.png           | Data Loss Warning / Upsell Dialog                 | commercial gate               | The application uses an aggressive modal overlay to interrupt the user workflow for upselling purposes, uti... |
| recovery-retention-gate-recurrent.png | Data Loss Warning / Upsell Dialog                 | commercial gate (recurrent)   | The application uses an intrusive modal upsell strategy that interrupts the user's primary task of data rec... |
| recovery-retention-gate-v2.png        | Recovery Results / Upsell Dialog                  | commercial gate (v2)          | The UI uses a high-urgency upsell modal overlaying the results to encourage purchase before data is potenti... |
| retention-gate-landing-nag.png        | Data Loss Warning Dialog                          | commercial gate (landing nag) | The application uses an aggressive modal interruption strategy to warn about data loss while simultaneously... |

## Cross-check notes (VLM vs hard evidence)

- VLM accent-blue estimates (#3b82f6 / #1677ff / #2563eb) bracket the hard-evidence brand
  blue `#0560fd` (themdark.ini slot 6). Hard evidence wins; VLM hex precision is ±10%.
- VLM radius census (4/6/8/12/20) matches the QSS census (4/6/8/12/16 + pill 20).
- VLM type census (13–14px body, 600 weights, 20–28px hero) matches QSS (14px base,
  12/13/15/16 secondary scale, 600 Semibold).
- VLM control heights (32/36/40/44) reconcile with QSS min-height ladder (16/19/24/32
  small controls; 36–40px primary CTAs at 14px font).
- Icon sizes: 16/20/24/32/48 observed — matches the 24/48 icon standard with 16–20 title
  bar glyphs and 48+ feature art.
