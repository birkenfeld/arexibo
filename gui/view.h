#ifndef AREXIBO_VIEW_H
#define AREXIBO_VIEW_H

#include <QMainWindow>
#include <QtWebEngineWidgets/QWebEngineView>
#include <QtWebChannel/QWebChannel>
#include <iostream>
#include <cstdint>

typedef void (*layout_callback)(void *, ssize_t which);
typedef void (*screenshot_callback)(void *, const char *data, ssize_t len);

class Window : public QMainWindow
{
    Q_OBJECT
    friend class JSInterface;

public:
    Window(QString, int, void *, void *, void *);

private:
    QWebEngineView *view;
    QWebChannel *channel;
    QString base_uri;

    void *cb_ptr;
    layout_callback layout_cb;
    screenshot_callback shot_cb;

signals:
    void navigateTo(QString);
    void screenShot();
    void setTitle(QString);
    void setSize(int, int, int, int);
    void setScale(int, int);
    void runJavascript(QString);

public slots:
    void navigateToImpl(QString);
    void screenShotImpl();
    void setSizeImpl(int, int, int, int);
    void setScaleImpl(int, int);
    void runJavascriptImpl(QString);
};

class JSInterface : public QObject
{
    Q_OBJECT

public:
    JSInterface(Window *wnd) : QObject(wnd), wnd(wnd) {}

private:
    Window *wnd;

public slots:
    void jsConnected();
    void jsLayoutDone();
    void jsLayoutPrev();
    void jsLayoutJump(int which);
};

#endif
