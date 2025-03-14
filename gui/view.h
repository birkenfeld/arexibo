#ifndef AREXIBO_VIEW_H
#define AREXIBO_VIEW_H

#include <QMainWindow>
#include <QtWebEngineWidgets/QWebEngineView>
#include <QtWebChannel/QWebChannel>
#include <iostream>
#include <cstdint>

typedef void (*layoutdone_callback)(void *);
typedef void (*screenshot_callback)(void *, const char *data, ssize_t len);

class Window : public QMainWindow
{
    Q_OBJECT

public:
    Window(QString, int, void *, void *, void *);

private:
    QWebEngineView *view;
    QWebChannel *channel;
    QString base_uri;
    void *cb_ptr;
    layoutdone_callback done_cb;
    screenshot_callback shot_cb;

signals:
    void navigateTo(QString);
    void screenShot();
    void setSettings(QString, int, int, int, int, int, int);

public slots:
    void navigateToImpl(QString);
    void screenShotImpl();
    void setSettingsImpl(QString, int, int, int, int, int, int);

    void jsConnected();
    void jsStartPlay(int);
    void jsLayoutDone();
};

#endif
