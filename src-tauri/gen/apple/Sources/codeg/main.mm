#include "bindings/bindings.h"

#import <UIKit/UIKit.h>

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(strong, nonatomic) UIWindow *window;
@end

@implementation SceneDelegate

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
  (void)scene;
  (void)session;

  for (UIOpenURLContext *context in connectionOptions.URLContexts) {
    [self scene:scene openURLContexts:[NSSet setWithObject:context]];
  }

  NSUserActivity *userActivity = connectionOptions.userActivities.anyObject;
  if (userActivity != nil) {
    [self scene:scene continueUserActivity:userActivity];
  }
}

- (void)sceneDidBecomeActive:(UIScene *)scene {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(applicationDidBecomeActive:)]) {
    [delegate applicationDidBecomeActive:UIApplication.sharedApplication];
  }
}

- (void)sceneWillResignActive:(UIScene *)scene {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(applicationWillResignActive:)]) {
    [delegate applicationWillResignActive:UIApplication.sharedApplication];
  }
}

- (void)sceneWillEnterForeground:(UIScene *)scene {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(applicationWillEnterForeground:)]) {
    [delegate applicationWillEnterForeground:UIApplication.sharedApplication];
  }
}

- (void)sceneDidEnterBackground:(UIScene *)scene {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(applicationDidEnterBackground:)]) {
    [delegate applicationDidEnterBackground:UIApplication.sharedApplication];
  }
}

- (void)scene:(UIScene *)scene openURLContexts:(NSSet<UIOpenURLContext *> *)URLContexts {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  for (UIOpenURLContext *context in URLContexts) {
    if ([delegate respondsToSelector:@selector(application:openURL:options:)]) {
      [delegate application:UIApplication.sharedApplication
                    openURL:context.URL
                    options:@{}];
    }
  }
}

- (void)scene:(UIScene *)scene continueUserActivity:(NSUserActivity *)userActivity {
  (void)scene;
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(application:continueUserActivity:restorationHandler:)]) {
    [delegate application:UIApplication.sharedApplication
        continueUserActivity:userActivity
          restorationHandler:^(NSArray<id<UIUserActivityRestoring>> *restorableObjects) {
            (void)restorableObjects;
          }];
  }
}

@end

int main(int argc, char * argv[]) {
	ffi::start_app();
	return 0;
}
