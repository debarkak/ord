#define _DEFAULT_SOURCE
#define _POSIX_C_SOURCE 199309L
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <time.h>
#include <string.h>
#include <X11/Xlib.h>
#include <X11/extensions/Xrandr.h>

int main(int argc, char **argv) {
    Display *d = XOpenDisplay(":0");
    if (!d) {
        fprintf(stderr, "Cannot open display :0\n");
        return 1;
    }

    int screen = DefaultScreen(d);
    Window root = RootWindow(d, screen);

    int target_x = 3072;
    int target_y = 0;

    // Retry finding secondary monitor
    for (int retry = 0; retry < 20; retry++) {
        XRRScreenResources *res = XRRGetScreenResources(d, root);
        if (res) {
            for (int i = 0; i < res->noutput; i++) {
                XRROutputInfo *info = XRRGetOutputInfo(d, res, res->outputs[i]);
                if (info && info->crtc && info->connection == RR_Connected) {
                    XRRCrtcInfo *crtc = XRRGetCrtcInfo(d, res, info->crtc);
                    if (crtc && (crtc->x > 0 || strcmp(info->name, "eDP-1") != 0)) {
                        target_x = crtc->x;
                        target_y = crtc->y;
                        printf("Found secondary monitor %s at (%d, %d)\n", info->name, target_x, target_y);
                        XRRFreeCrtcInfo(crtc);
                        XRRFreeOutputInfo(info);
                        XRRFreeScreenResources(res);
                        goto found;
                    }
                    if (crtc) XRRFreeCrtcInfo(crtc);
                }
                if (info) XRRFreeOutputInfo(info);
            }
            XRRFreeScreenResources(res);
        }
        usleep(50000); // 50ms
    }

found:
    XSetWindowAttributes attr;
    attr.override_redirect = True;
    attr.background_pixel = 0;

    Window win = XCreateWindow(
        d, root,
        target_x, target_y, 2, 2, 0,
        CopyFromParent, InputOutput, CopyFromParent,
        CWOverrideRedirect | CWBackPixel, &attr
    );

    XMapRaised(d, win);
    XFlush(d);

    printf("ORD X11 Micro-Damage Driver active on secondary monitor at (%d, %d) at 165 FPS\n", target_x, target_y);

    struct timespec ts = {0, 6060606}; // 6.06ms (165 FPS)
    int tick = 0;
    while (1) {
        nanosleep(&ts, NULL);
        XClearArea(d, win, 0, 0, 2, 2, True);
        XFlush(d);

        if (++tick % 165 == 0) {
            XRRScreenResources *res = XRRGetScreenResources(d, root);
            if (res) {
                for (int i = 0; i < res->noutput; i++) {
                    XRROutputInfo *info = XRRGetOutputInfo(d, res, res->outputs[i]);
                    if (info && info->crtc && info->connection == RR_Connected) {
                        XRRCrtcInfo *crtc = XRRGetCrtcInfo(d, res, info->crtc);
                        if (crtc && (crtc->x > 0 || strcmp(info->name, "eDP-1") != 0)) {
                            if (crtc->x != target_x || crtc->y != target_y) {
                                target_x = crtc->x;
                                target_y = crtc->y;
                                XMoveWindow(d, win, target_x, target_y);
                                printf("Updated secondary monitor position to (%d, %d)\n", target_x, target_y);
                            }
                            XRRFreeCrtcInfo(crtc);
                            XRRFreeOutputInfo(info);
                            break;
                        }
                        if (crtc) XRRFreeCrtcInfo(crtc);
                    }
                    if (info) XRRFreeOutputInfo(info);
                }
                XRRFreeScreenResources(res);
            }
        }
    }

    XDestroyWindow(d, win);
    XCloseDisplay(d);
    return 0;
}
