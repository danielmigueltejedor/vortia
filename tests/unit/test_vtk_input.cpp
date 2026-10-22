#include "vortia/visualization/VtkInteractorStyle.hpp"

#include <gtest/gtest.h>
#include <vtkNew.h>

TEST(VtkInteractorInput, HoverCannotArmCameraWithoutQtPress) {
    vtkNew<vortia::VtkInteractorStyle> style;
    EXPECT_FALSE(style->qtDragAuthorized());
    style->OnLeftButtonDown();
    style->OnMouseMove();
    EXPECT_FALSE(style->qtDragAuthorized());
}

TEST(VtkInteractorInput, PointerReleaseOrFocusLossDisarmsCamera) {
    vtkNew<vortia::VtkInteractorStyle> style;
    style->setQtDragAuthorized(true);
    EXPECT_TRUE(style->qtDragAuthorized());
    style->setQtDragAuthorized(false);
    style->OnMouseMove();
    EXPECT_FALSE(style->qtDragAuthorized());
}

TEST(VtkInteractorInput, UnsolicitedMiddleOrRightButtonsDoNotArmCamera) {
    vtkNew<vortia::VtkInteractorStyle> style;
    style->OnMiddleButtonDown();
    style->OnRightButtonDown();
    EXPECT_FALSE(style->qtDragAuthorized());
}
