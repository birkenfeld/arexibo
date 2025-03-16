#include <QApplication>
#include <QMainWindow>
#include <QtWebEngineCore/QWebEngineProfile>
#include <QtWebEngineCore/QWebEngineSettings>
#include <QtWebEngineWidgets/QWebEngineView>

#include "lib.h"
#include "view.h"

// For some reason, this constructor is not automatically called
int qInitResources_res();

QApplication *the_app = nullptr;
Window *the_wnd = nullptr;

int fake_argc = 1;
char *fake_argv[] = {(char *)"arexibo", nullptr};

void setup(const char *base_uri, int inspect,
           void *callback_ptr, void *done_cb, void *shot_cb) {
    if (the_wnd) return;

    qInitResources_res();

    QCoreApplication::setOrganizationName("arexibo");
    the_app = new QApplication(fake_argc, fake_argv);

    QWebEngineProfile::defaultProfile()->settings()->setAttribute(
        QWebEngineSettings::ScreenCaptureEnabled, true);
    QWebEngineProfile::defaultProfile()->settings()->setAttribute(
        QWebEngineSettings::PlaybackRequiresUserGesture, false);

    the_wnd = new Window(base_uri, inspect, callback_ptr, done_cb, shot_cb);
    the_wnd->show();
}

void run() {
    if (!the_app) return;
    the_app->exec();
}

void navigate(const char *url) {
    if (!the_wnd) return;
    emit the_wnd->navigateTo(url);
}

void screenshot() {
    if (!the_wnd) return;
    emit the_wnd->screenShot();
}

void set_title(const char *title) {
    if (!the_wnd) return;
    emit the_wnd->setTitle(title);
}

void set_size(int pos_x, int pos_y, int size_x, int size_y) {
    if (!the_wnd) return;
    emit the_wnd->setSize(pos_x, pos_y, size_x, size_y);
}

void set_scale(int layout_w, int layout_h) {
    if (!the_wnd) return;
    emit the_wnd->setScale(layout_w, layout_h);
}
