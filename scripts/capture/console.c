/* WebKit release builds otherwise hide JS errors needed to reject invalid captures. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <webkit2/webkit2.h>

WebKitSettings *webkit_web_view_get_settings(WebKitWebView *view) {
    static WebKitSettings *(*original)(WebKitWebView *);
    if (!original) original = dlsym(RTLD_NEXT, "webkit_web_view_get_settings");
    WebKitSettings *settings = original(view);
    g_object_set(settings, "enable-write-console-messages-to-stdout", TRUE, NULL);
    return settings;
}
