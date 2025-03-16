#include <QApplication>
#include <QScreen>
#include <QIODevice>
#include <QBuffer>

#include "view.h"

Window::Window(QString base_uri, int inspect, void *cb_ptr,
               void *done_cb, void *shot_cb) :
    QMainWindow(),
    base_uri(base_uri),
    cb_ptr(cb_ptr),
    done_cb((layoutdone_callback)done_cb),
    shot_cb((screenshot_callback)shot_cb)
{
    setWindowFlags(windowFlags() | Qt::FramelessWindowHint);
    setWindowIcon(QIcon(":/assets/logo.png"));

    view = new QWebEngineView(this);

    channel = new QWebChannel(this);
    view->page()->setWebChannel(channel);
    channel->registerObject("arexibo", this);

    if (inspect) {
        auto devtools_window = new QMainWindow();
        auto devtools = new QWebEngineView();
        devtools_window->setWindowTitle("Arexibo - Inspector");
        devtools_window->setWindowIcon(QIcon(":/assets/logo.png"));
        devtools_window->setCentralWidget(devtools);
        devtools_window->resize(1000, 600);
        devtools_window->show();
        view->page()->setDevToolsPage(devtools->page());
    } else {
        QGuiApplication::setOverrideCursor(Qt::BlankCursor);        
    }

    connect(this, SIGNAL(navigateTo(QString)), this, SLOT(navigateToImpl(QString)));
    connect(this, SIGNAL(screenShot()), this, SLOT(screenShotImpl()));
    connect(this, SIGNAL(setTitle(QString)), this, SLOT(setWindowTitle(QString)));
    connect(this, SIGNAL(setSize(int, int, int, int)),
            this, SLOT(setSizeImpl(int, int, int, int)));
    connect(this, SIGNAL(setScale(int, int)), this, SLOT(setScaleImpl(int, int)));

    view->setUrl(QUrl(base_uri + "0.xlf.html"));
}

void Window::navigateToImpl(QString url) {
    view->setUrl(QUrl(base_uri + url));
}

void Window::screenShotImpl()
{
    QImage img(view->size(), QImage::Format_ARGB32);
    view->render(&img);
    QByteArray array;
    QBuffer buffer(&array);
    buffer.open(QIODevice::WriteOnly);
    img.save(&buffer, "PNG");
    shot_cb(cb_ptr, array, array.size());
}

void Window::setSizeImpl(int pos_x, int pos_y, int size_x, int size_y)
{
    // find current screen size
    QRect screenGeometry = screen()->geometry();
    int screen_w = screenGeometry.width();
    int screen_h = screenGeometry.height();

    // calculate window position and size
    if (size_x == 0 && size_y == 0 && pos_x == 0 && pos_y == 0) {
        size_x = screen_w;
        size_y = screen_h;
        showFullScreen();
    } else {
        if (size_x == 0) size_x = screen_w;
        if (size_y == 0) size_y = screen_h;
        setWindowState(windowState() ^ Qt::WindowFullScreen);
        resize(size_x, size_y);
        move(pos_x, pos_y);
    }
}

void Window::setScaleImpl(int layout_w, int layout_h)
{
    int window_w = width();
    int window_h = height();

    // the easy case: direct match
    if (window_w == layout_w && window_h == layout_h) {
        view->move(0, 0);
        view->resize(layout_w, layout_h);
        view->setZoomFactor(1.0);
        return;
    }

    // nothing specified for the layout (e.g. splash)?
    if (layout_w == 0 || layout_h == 0) {
        layout_w = 1920;
        layout_h = 1080;
    }

    // adjust position of webview within the window, and apply the scale
    double window_aspect = (double)window_w / (double)window_h;
    double layout_aspect = (double)layout_w / (double)layout_h;
    if (window_aspect > layout_aspect) {
        double scale_factor = (double)window_h / (double)layout_h;
        int webview_w = (int)((double)layout_w * scale_factor);
        view->move((window_w - webview_w) / 2, 0);
        view->resize(webview_w, window_h);
        view->setZoomFactor(scale_factor);
    } else {
        double scale_factor = (double)window_w / (double)layout_w;
        int webview_h = (int)((double)layout_h * scale_factor);
        view->move(0, (window_h - webview_h) / 2);
        view->resize(window_w, webview_h);
        view->setZoomFactor(scale_factor);
    }
}

// Callbacks from JavaScript

void Window::jsLayoutDone()
{
    done_cb(cb_ptr);
}

void Window::jsConnected()
{
    std::cout << "WebChannel is connected" << std::endl;
}
