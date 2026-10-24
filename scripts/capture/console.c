/* Capture-only WebKit launch settings; native Tauri internals stay immutable. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <webkit2/webkit2.h>

WebKitSettings *webkit_web_view_get_settings(WebKitWebView *view) {
    static WebKitSettings *(*original)(WebKitWebView *);
    if (!original) original = dlsym(RTLD_NEXT, "webkit_web_view_get_settings");
    WebKitSettings *settings = original(view);
    g_object_set(settings, "enable-write-console-messages-to-stdout", TRUE, NULL);
    if (!g_object_get_data(G_OBJECT(view), "capture-style")) {
        const char *css = g_getenv("BLOCKFIELD_CAPTURE_STATIC")
            ? "* {caret-color:transparent !important;animation-play-state:paused !important;transition:none !important;}"
            : "* {caret-color:transparent !important;}";
        WebKitUserStyleSheet *sheet = webkit_user_style_sheet_new(css,
            WEBKIT_USER_CONTENT_INJECT_ALL_FRAMES, WEBKIT_USER_STYLE_LEVEL_USER, NULL, NULL);
        webkit_user_content_manager_add_style_sheet(webkit_web_view_get_user_content_manager(view), sheet);
        webkit_user_style_sheet_unref(sheet);
        gchar *probe = NULL;
        if (g_file_get_contents("/harness/progress-ready.js", &probe, NULL, NULL)) {
            WebKitUserScript *script = webkit_user_script_new(probe,
                WEBKIT_USER_CONTENT_INJECT_TOP_FRAME, WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_END, NULL, NULL);
            webkit_user_content_manager_add_script(webkit_web_view_get_user_content_manager(view), script);
            webkit_user_script_unref(script);
            g_free(probe);
        }
        g_object_set_data(G_OBJECT(view), "capture-style", GINT_TO_POINTER(1));
    }
    return settings;
}

void webkit_web_view_load_uri(WebKitWebView *view, const gchar *uri) {
    static void (*original)(WebKitWebView *, const gchar *);
    if (!original) original = dlsym(RTLD_NEXT, "webkit_web_view_load_uri");
    const char *query = g_getenv("BLOCKFIELD_CAPTURE_QUERY");
    if (query && g_str_has_prefix(uri, "tauri://localhost") && !strchr(uri, '?')) {
        gchar *selected = g_strdup_printf("%s?%s", uri, query);
        original(view, selected);
        g_free(selected);
    } else {
        original(view, uri);
    }
}
