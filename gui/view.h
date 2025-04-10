#ifndef AREXIBO_VIEW_H
#define AREXIBO_VIEW_H

#include <QMainWindow>
#include <QtWebEngineWidgets/QWebEngineView>
#include <QtWebChannel/QWebChannel>
#include <iostream>

#include "lib.h"

class Window : public QMainWindow
{
    Q_OBJECT
    friend class JSInterface;

public:
    Window(QString, int, callback, void *);

private:
    QWebEngineView *view;
    QWebChannel *channel;
    QString base_uri;

    callback cb;
    void *cb_ptr;

    int layout_width;
    int layout_height;

    void adjustScale(int, int);

signals:
    void navigateTo(QString);
    void screenShot();
    void setTitle(QString);
    void setSize(int, int, int, int);
    void runJavascript(QString);

public slots:
    void navigateToImpl(QString);
    void screenShotImpl();
    void setSizeImpl(int, int, int, int);
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
    void jsLayoutInit(int, int, int);
    void jsLayoutDone(int);
    void jsLayoutPrev(int);
    void jsLayoutJump(int, int);
    void jsCommand(QString);
    void jsShell(QString, int);
    void jsStopShell(int);
};

#endif
