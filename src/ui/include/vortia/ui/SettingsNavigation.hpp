#pragma once
#include <QWidget>
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
        setObjectName(name+"-sidebar");setFixedWidth(210);group_=new QButtonGroup(this);group_->setObjectName(name);group_->setExclusive(true);
        layout_=new QVBoxLayout(this);layout_->setContentsMargins(0,0,0,0);layout_->setSpacing(8);layout_->addStretch();
        setStyleSheet("QPushButton {text-align:left; background:transparent; border:1px solid transparent; border-radius:9px; padding:12px; min-height:28px;} QPushButton:hover {background:palette(midlight);} QPushButton:checked {background:#246b8e; color:white;} QPushButton:focus {border:1px solid #63b2e6;}");
        QObject::connect(group_,&QButtonGroup::idToggled,pages_,[pages](int index,bool checked){if(checked)pages->setCurrentIndex(index);});
    }
    void addPage(const QString& title,const QString& icon,const QString& accessibleTitle={}) {
        const int index=group_->buttons().size();auto* button=new QPushButton(title,this);
        button->setObjectName(group_->objectName()+"-page-"+QString::number(index));button->setCheckable(true);button->setAutoDefault(false);
        button->setAccessibleName(accessibleTitle.isEmpty()?title:accessibleTitle);button->setToolTip(button->accessibleName());
        QIcon glyph(icon);auto on=glyph.pixmap(26,26);if(!on.isNull()){QPainter paint(&on);paint.setCompositionMode(QPainter::CompositionMode_SourceIn);paint.fillRect(on.rect(),Qt::white);paint.end();glyph.addPixmap(on,QIcon::Normal,QIcon::On);}
        button->setIcon(glyph);button->setIconSize({26,26});button->setShortcut(QKeySequence(QString("Alt+%1").arg(index+1)));
        group_->addButton(button,index);layout_->insertWidget(index,button);
    }
    void selectPage(int index) {if(auto* b=group_->button(index)){b->setChecked(true);pages_->setCurrentIndex(index);}}
};
}
