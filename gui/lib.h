#ifndef AREXIBO_LIB_H
#define AREXIBO_LIB_H

extern "C" {

void setup(const char *base_uri, int inspect,
           void *cb_ptr, void *done_cb, void *shot_cb);
void run();
void navigate(const char *url);
void screenshot();
void set_settings(const char *title, int pos_x, int pos_y, int size_x, int size_y,
                  int layout_w, int layout_h);

}

#endif
