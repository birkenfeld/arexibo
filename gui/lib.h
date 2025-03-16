#ifndef AREXIBO_LIB_H
#define AREXIBO_LIB_H

extern "C" {

void setup(const char *base_uri, int inspect,
           void *cb_ptr, void *done_cb, void *shot_cb);
void run();
void navigate(const char *file);
void screenshot();
void set_title(const char *title);
void set_size(int pos_x, int pos_y, int size_x, int size_y);
void set_scale(int layout_w, int layout_h);

}

#endif
