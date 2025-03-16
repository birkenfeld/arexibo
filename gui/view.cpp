#include <QApplication>
#include <QScreen>
#include <QIODevice>
#include <QBuffer>

#include "view.h"

Window::Window(QString base_uri, int inspect, void *cb_ptr, void *done_cb, void *shot_cb) :
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
    auto interface = new JSInterface(this);
    channel->registerObject("arexibo", interface);

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

void Window::navigateToImpl(QString file) {
    view->setUrl(QUrl(base_uri + file));
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

    if (size_x == 0) size_x = screen_w;
    if (size_y == 0) size_y = screen_h;

    // calculate window position and size
    if (size_x == screen_w && size_y == screen_h && pos_x == 0 && pos_y == 0) {
        resize(size_x, size_y);
        move(0, 0);
        showFullScreen();
        std::cout << "INFO : [arexibo::qt] size: full screen" << std::endl;
    } else {
        setWindowState(windowState() & ~Qt::WindowFullScreen);
        resize(size_x, size_y);
        move(pos_x, pos_y);
        std::cout << "INFO : [arexibo::qt] size: windowed ("
                  << size_x << "x" << size_y << ")+"
                  << pos_x << "+" << pos_y << std::endl;
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
        std::cout << "INFO : [arexibo::qt] scale: window = layout ("
                  << layout_w << "x" << layout_h << ")" << std::endl;
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
    double scale_factor;
    if (window_aspect > layout_aspect) {
        scale_factor = (double)window_h / (double)layout_h;
        int webview_w = (int)((double)layout_w * scale_factor);
        view->move((window_w - webview_w) / 2, 0);
        view->resize(webview_w, window_h);
        view->setZoomFactor(scale_factor);
    } else {
        scale_factor = (double)window_w / (double)layout_w;
        int webview_h = (int)((double)layout_h * scale_factor);
        view->move(0, (window_h - webview_h) / 2);
        view->resize(window_w, webview_h);
        view->setZoomFactor(scale_factor);
    }
    std::cout << "INFO : [arexibo::qt] scale: window ("
              << window_w << "x" << window_h << "), layout ("
              << layout_w << "x" << layout_h << "), result: ("
              << view->width() << "x" << view->height() << ")+"
              << view->x() << "+" << view->y()
              << " with zoom " << scale_factor << std::endl;
}

// Callbacks from JavaScript

void JSInterface::jsLayoutDone()
{
    wnd->done_cb(wnd->cb_ptr);
}

void JSInterface::jsConnected()
{
    std::cout << "INFO : [arexibo::qt] WebChannel is connected" << std::endl;
}
