# Linux Virtual Display Backend

## Overview
Traditional screen-sharing applications capture an existing physical display and mirror its contents. In contrast, ORD creates an actual **virtual monitor** inside the Linux desktop compositor.

## GNOME Mutter Wayland Integration

GNOME 40+ (and GNOME 50+ on EndeavourOS / Arch Linux) provides native support for virtual monitors via D-Bus:

1. **D-Bus Interface**: `org.gnome.Mutter.ScreenCast`
2. **Method**: `CreateSession(a{sv})` → returns session path `/org/gnome/Mutter/ScreenCast/Session/uX`
3. **Method**: `RecordVirtual(a{sv})` → creates the monitor and returns stream path `/org/gnome/Mutter/ScreenCast/Stream/uY`
4. **Signal**: `PipeWireStreamAdded(u32 node_id)` → provides the PipeWire source node ID.
5. **Method**: `Start()` → activates rendering to the virtual display.

### Display Configuration
When the virtual monitor is spawned, it appears in GNOME Settings under **Displays**:
- The user can arrange it to the left, right, top, or bottom of the primary laptop screen.
- Windows can be dragged onto the virtual monitor.
- Mutter renders into the virtual display buffer and emits frames to PipeWire.

### Teardown & Ghost Monitor Prevention
When the Android client disconnects or if the host daemon is terminated (`SIGINT` / `SIGTERM`), ORD immediately calls `Stop()` on the ScreenCast session. Mutter automatically destroys the virtual monitor and restores the single-display configuration.
