#define _POSIX_C_SOURCE 199309L
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>
#include <sys/mman.h>
#include <wayland-client.h>
#include "xdg-shell-client-protocol.h"

static struct wl_display *display = NULL;
static struct wl_compositor *compositor = NULL;
static struct wl_shm *shm = NULL;
static struct xdg_wm_base *wm_base = NULL;
static struct wl_output *target_output = NULL;
static char target_output_name[64] = {0};

static void xdg_wm_base_ping(void *data, struct xdg_wm_base *shell, uint32_t serial) {
    xdg_wm_base_pong(shell, serial);
}
static const struct xdg_wm_base_listener wm_base_listener = {
    xdg_wm_base_ping,
};

static void output_name(void *data, struct wl_output *output, const char *name) {
    if (strcmp(name, "eDP-1") != 0 && target_output == NULL) {
        printf("Found target monitor: %s\n", name);
        target_output = output;
        strncpy(target_output_name, name, sizeof(target_output_name) - 1);
    }
}
static void output_geometry(void *d, struct wl_output *o, int32_t x, int32_t y, int32_t w, int32_t h, int32_t sp, const char *m, const char *mo, int32_t t) {}
static void output_mode(void *d, struct wl_output *o, uint32_t f, int32_t w, int32_t h, int32_t r) {}
static void output_done(void *d, struct wl_output *o) {}
static void output_scale(void *d, struct wl_output *o, int32_t s) {}
static void output_description(void *d, struct wl_output *o, const char *desc) {}

static const struct wl_output_listener output_listener = {
    output_geometry, output_mode, output_done, output_scale, output_name, output_description
};

static void registry_global(void *data, struct wl_registry *registry, uint32_t id, const char *interface, uint32_t version) {
    if (strcmp(interface, "wl_compositor") == 0) {
        compositor = wl_registry_bind(registry, id, &wl_compositor_interface, 4);
    } else if (strcmp(interface, "wl_shm") == 0) {
        shm = wl_registry_bind(registry, id, &wl_shm_interface, 1);
    } else if (strcmp(interface, "xdg_wm_base") == 0) {
        wm_base = wl_registry_bind(registry, id, &xdg_wm_base_interface, 1);
        xdg_wm_base_add_listener(wm_base, &wm_base_listener, NULL);
    } else if (strcmp(interface, "wl_output") == 0) {
        struct wl_output *out = wl_registry_bind(registry, id, &wl_output_interface, 4);
        wl_output_add_listener(out, &output_listener, NULL);
    }
}
static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t id) {}

static const struct wl_registry_listener registry_listener = {
    registry_global, registry_global_remove
};

static void xdg_surface_configure(void *data, struct xdg_surface *xdg_surface, uint32_t serial) {
    xdg_surface_ack_configure(xdg_surface, serial);
}
static const struct xdg_surface_listener xdg_surface_listener = {
    xdg_surface_configure,
};

static void xdg_toplevel_configure(void *d, struct xdg_toplevel *t, int32_t w, int32_t h, struct wl_array *s) {}
static void xdg_toplevel_close(void *d, struct xdg_toplevel *t) {}
static const struct xdg_toplevel_listener toplevel_listener = {
    xdg_toplevel_configure,
    xdg_toplevel_close,
};

int main(int argc, char **argv) {
    display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "Failed to connect to Wayland display\n");
        return 1;
    }

    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    wl_display_roundtrip(display);

    if (!compositor || !shm || !wm_base) {
        fprintf(stderr, "Compositor, SHM, or xdg_wm_base not available\n");
        return 1;
    }

    // Allocate 2x2 ARGB8888 SHM buffer (nearly invisible 2-pixel micro-dot)
    int fd = memfd_create("ord_pulse", MFD_CLOEXEC);
    ftruncate(fd, 16);
    uint32_t *pixels = mmap(NULL, 16, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    pixels[0] = 0x01000000;
    pixels[1] = 0x01000000;
    pixels[2] = 0x01000000;
    pixels[3] = 0x01000000;

    struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, 16);
    struct wl_buffer *buffer = wl_shm_pool_create_buffer(pool, 0, 2, 2, 8, WL_SHM_FORMAT_ARGB8888);
    wl_shm_pool_destroy(pool);
    close(fd);

    struct wl_surface *surface = wl_compositor_create_surface(compositor);
    struct xdg_surface *xdg_surf = xdg_wm_base_get_xdg_surface(wm_base, surface);
    xdg_surface_add_listener(xdg_surf, &xdg_surface_listener, NULL);

    struct xdg_toplevel *toplevel = xdg_surface_get_toplevel(xdg_surf);
    xdg_toplevel_add_listener(toplevel, &toplevel_listener, NULL);
    xdg_toplevel_set_title(toplevel, "ord-ticker");
    xdg_toplevel_set_app_id(toplevel, "org.ord.ticker");
    xdg_toplevel_set_min_size(toplevel, 1, 1);
    xdg_toplevel_set_max_size(toplevel, 2, 2);

    // Make surface 100% click-through (empty input region)
    struct wl_region *empty_input = wl_compositor_create_region(compositor);
    wl_surface_set_input_region(surface, empty_input);
    wl_region_destroy(empty_input);

    // Initial commit
    wl_surface_attach(surface, buffer, 0, 0);
    wl_surface_damage_buffer(surface, 0, 0, 2, 2);
    wl_surface_commit(surface);
    wl_display_roundtrip(display);

    printf("ORD Native Wayland Micro-Damage Driver running (2x2 pixel, 100%% transparent wallpaper visible)!\n");

    struct timespec ts = {0, 11111111}; // 11.1ms (90 FPS)
    while (1) {
        nanosleep(&ts, NULL);
        pixels[0] = (pixels[0] == 0x01000000) ? 0x02000000 : 0x01000000;
        wl_surface_attach(surface, buffer, 0, 0);
        wl_surface_damage_buffer(surface, 0, 0, 2, 2);
        wl_surface_commit(surface);
        wl_display_flush(display);
    }

    return 0;
}
