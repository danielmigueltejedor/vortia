#pragma once
#include <QWidget>
#include "vortia/ui/UiIcon.hpp"
#include <QButtonGroup>
#include <QPushButton>
#include <QStackedWidget>
#include <QVBoxLayout>
#include <QIcon>
#include <QPainter>
namespace vortia {
// Real buttons expose each category to keyboard and native accessibility APIs.
// Short visible titles and full accessible names avoid clipped sidebar labels.
class SettingsNavigation final:public QWidget {
    QStackedWidget* pages_;QButtonGroup* group_;QVBoxLayout* layout_;
public:
    SettingsNavigation(QWidget* parent,QStackedWidget* pages,const QString& name):QWidget(parent),pages_(pages) {
        setObjectName(name+"-sidebar");setFixedWidth(180);group_=new QButtonGroup(this);group_->setObjectName(name);group_->setExclusive(true);
        layout_=new QVBoxLayout(this);layout_->setContentsMargins(0,0,0,0);layout_->setSpacing(4);layout_->addStretch();

        QObject::connect(group_,&QButtonGroup::idToggled,pages_,[pages](int index,bool checked){if(checked)pages->setCurrentIndex(index);});
    }
    void addPage(const QString& title,const QString& icon,const QString& accessibleTitle={}) {
        const int index=group_->buttons().size();auto* button=new QPushButton(title,this);
        button->setObjectName(group_->objectName()+"-page-"+QString::number(index));button->setCheckable(true);button->setAutoDefault(false);
        button->setAccessibleName(accessibleTitle.isEmpty()?title:accessibleTitle);button->setToolTip(button->accessibleName());
        const auto glyph=uiIcon(icon);
        button->setIcon(glyph);button->setIconSize({20,20});button->setShortcut(QKeySequence(QString("Alt+%1").arg(index+1)));
        group_->addButton(button,index);layout_->insertWidget(index,button);
    }
    void selectPage(int index) {if(auto* b=group_->button(index)){b->setChecked(true);pages_->setCurrentIndex(index);}}
};
}
